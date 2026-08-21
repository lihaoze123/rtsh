use std::{
    ffi::CString,
    io::{self, Write},
    os::{fd::AsFd, raw::c_void},
    println,
};

use nix::{
    errno::Errno,
    libc,
    poll::{PollFd, PollFlags, PollTimeout, poll},
    sys::{
        signal::Signal,
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
    unistd::{ForkResult, Pid, execvp, fork, read, setpgid},
};

use crate::{
    jobs::{Job, JobState, JobTable},
    parser::{CommandKind, ParseCommandError, ParsedCommand},
    signal::SignalSource,
};

#[derive(Default)]
struct InputBuffer {
    bytes: Vec<u8>,
    eof: bool,
}

impl InputBuffer {
    fn take_line(&mut self) -> anyhow::Result<Option<String>> {
        let end = match self.bytes.iter().position(|&byte| byte == b'\n') {
            Some(index) => index + 1,
            None if self.eof && !self.bytes.is_empty() => self.bytes.len(),
            None => return Ok(None),
        };
        let line = self.bytes.drain(..end).collect::<Vec<_>>();
        Ok(Some(String::from_utf8(line)?))
    }
}

pub struct Shell {
    signals: SignalSource,
    jobs: JobTable,
}

impl Shell {
    pub fn new(signals: SignalSource) -> Self {
        Self {
            signals,
            jobs: JobTable::new(),
        }
    }

    fn eval(&mut self, line: &str) -> anyhow::Result<bool> {
        let command = match line.parse::<ParsedCommand>() {
            Ok(command) => command,
            Err(ParseCommandError::EmptyCommand) => {
                return Ok(false);
            }
            Err(error) => {
                println!("{error}");
                return Ok(false);
            }
        };

        match command.kind() {
            CommandKind::Quit => Ok(true),
            CommandKind::External => {
                self.spawn_external(&command, line)?;
                Ok(false)
            }
            CommandKind::Jobs => {
                self.list_jobs();
                Ok(false)
            }
            CommandKind::Bg => {
                self.run_background(&command)?;
                Ok(false)
            }
            CommandKind::Fg => {
                self.run_foreground(&command)?;
                Ok(false)
            }
        }
    }

    fn spawn_external(&mut self, command: &ParsedCommand, cli: &str) -> anyhow::Result<()> {
        let argv = command
            .argv()
            .iter()
            .map(|arg| CString::new(arg.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        let prog = &argv[0];
        let background = command.is_background();

        let exec_error = format!("{}: Command not found\n", command.argv()[0]);
        match unsafe { fork()? } {
            ForkResult::Child => {
                if setpgid(Pid::from_raw(0), Pid::from_raw(0)).is_err() {
                    unsafe {
                        libc::_exit(1);
                    }
                }

                if self.signals.restore_mask_in_child().is_err() {
                    unsafe {
                        libc::_exit(1);
                    }
                }

                let _ = execvp(prog, &argv);
                unsafe {
                    libc::write(
                        libc::STDOUT_FILENO,
                        exec_error.as_ptr() as *const c_void,
                        exec_error.len(),
                    );
                    libc::_exit(127);
                }
            }
            ForkResult::Parent { child } => {
                match setpgid(child, child) {
                    Ok(()) | Err(Errno::EACCES) | Err(Errno::ESRCH) => {}
                    Err(error) => return Err(error.into()),
                }
                let state = if background {
                    JobState::Background
                } else {
                    JobState::Foreground
                };
                let job = self.jobs.add(child, child, state, cli.to_owned())?;
                if background {
                    println!("[{}] ({}) {}", job.jid, job.pid, cli.trim_end_matches('\n'));
                    Ok(())
                } else {
                    self.wait_foreground()
                }
            }
        }
    }

    fn drain_signals(&mut self) -> anyhow::Result<()> {
        while let Some(signal) = self.signals.next_signal()? {
            self.handle_signal(signal)?;
        }
        Ok(())
    }

    fn handle_signal(&mut self, signal: Signal) -> anyhow::Result<()> {
        match signal {
            Signal::SIGINT | Signal::SIGTSTP => {
                if let Some(pgid) = self.jobs.foreground_pgid() {
                    nix::sys::signal::killpg(pgid, signal)?;
                }
            }

            Signal::SIGCHLD => loop {
                match waitpid(
                    None,
                    Some(WaitPidFlag::WNOHANG | WaitPidFlag::WUNTRACED | WaitPidFlag::WCONTINUED),
                ) {
                    Ok(WaitStatus::Exited(pid, _)) => {
                        self.jobs.remove_by_pid(pid);
                    }

                    Ok(WaitStatus::Signaled(pid, signal, _)) => {
                        if let Some(job) = self.jobs.remove_by_pid(pid) {
                            println!(
                                "Job [{}] ({}) terminated by signal {}",
                                job.jid, job.pid, signal as i32,
                            );
                        }
                    }

                    Ok(WaitStatus::Stopped(pid, signal)) => {
                        if let Some(job) = self.jobs.set_state(pid, JobState::Stopped) {
                            println!(
                                "Job [{}] ({}) stopped by signal {}",
                                job.jid, job.pid, signal as i32,
                            );
                        }
                    }

                    Ok(WaitStatus::StillAlive) | Err(Errno::ECHILD) => break,

                    Ok(_) => {}
                    Err(error) => return Err(error.into()),
                }
            },

            Signal::SIGQUIT => {
                println!("Terminating after receipt of SIGQUIT signal");
                std::process::exit(1);
            }

            _ => {}
        }

        Ok(())
    }

    pub fn run(&mut self, emit_prompt: bool) -> anyhow::Result<()> {
        let stdin = io::stdin();
        let mut input = InputBuffer::default();

        loop {
            if emit_prompt {
                print!("tsh> ");
                io::stdout().flush()?;
            }

            let line = loop {
                if let Some(line) = input.take_line()? {
                    break line;
                }

                if input.eof {
                    return Ok(());
                }

                let (stdin_ready, signal_ready) = {
                    let mut fds = [
                        PollFd::new(stdin.as_fd(), PollFlags::POLLIN | PollFlags::POLLHUP),
                        PollFd::new(self.signals.as_fd(), PollFlags::POLLIN),
                    ];

                    poll(&mut fds, PollTimeout::NONE)?;

                    let stdin_events = fds[0].revents().unwrap_or_else(PollFlags::empty);
                    let signal_events = fds[1].revents().unwrap_or_else(PollFlags::empty);

                    (
                        stdin_events.intersects(PollFlags::POLLIN | PollFlags::POLLHUP),
                        signal_events.contains(PollFlags::POLLIN),
                    )
                };

                if signal_ready {
                    self.drain_signals()?;
                }

                if stdin_ready {
                    let mut chunk = [0u8; 4096];
                    match read(stdin.as_fd(), &mut chunk) {
                        Ok(0) => input.eof = true,
                        Ok(count) => {
                            input.bytes.extend_from_slice(&chunk[..count]);
                        }
                        Err(Errno::EINTR | Errno::EAGAIN) => {}
                        Err(error) => return Err(error.into()),
                    }
                }
            };

            if self.eval(&line)? {
                return Ok(());
            }
        }
    }

    fn wait_foreground(&mut self) -> anyhow::Result<()> {
        while self.jobs.foreground_pgid().is_some() {
            {
                let mut fds = [PollFd::new(self.signals.as_fd(), PollFlags::POLLIN)];
                poll(&mut fds, PollTimeout::NONE)?;
            }
            self.drain_signals()?;
        }
        Ok(())
    }

    fn list_jobs(&self) {
        for job in self.jobs.iter() {
            println!(
                "[{}] ({}) {} {}",
                job.jid,
                job.pid,
                job.state.display_name(),
                job.cli.trim_end_matches('\n'),
            );
        }
    }

    fn find_job(&mut self, command_name: &str, argument: &str) -> Option<&mut Job> {
        if let Some(raw_jid) = argument.strip_prefix('%') {
            let jid = match raw_jid.parse::<u32>() {
                Ok(jid) if jid > 0 => jid,
                _ => {
                    println!("{command_name}: argument must be a PID or %jobid");
                    return None;
                }
            };

            match self.jobs.get_mut_by_jid(jid) {
                Some(job) => Some(job),
                None => {
                    println!("{argument}: No such job");
                    None
                }
            }
        } else {
            let raw_pid = match argument.parse::<i32>() {
                Ok(pid) if pid > 0 => pid,
                _ => {
                    println!("{command_name}: argument must be a PID or %jobid");
                    return None;
                }
            };

            let pid = Pid::from_raw(raw_pid);
            match self.jobs.get_mut_by_pid(pid) {
                Some(job) => Some(job),
                None => {
                    println!("({argument}): No such process");
                    None
                }
            }
        }
    }

    fn run_background(&mut self, command: &ParsedCommand) -> anyhow::Result<()> {
        let Some(argument) = command.argv().get(1) else {
            println!("bg command requires PID or %jobid argument");
            return Ok(());
        };

        let Some(job) = self.find_job("bg", argument) else {
            return Ok(());
        };

        nix::sys::signal::killpg(job.pgid, Signal::SIGCONT)?;
        job.state = JobState::Background;

        println!(
            "[{}] ({}) {}",
            job.jid,
            job.pid,
            job.cli.trim_end_matches('\n'),
        );
        Ok(())
    }

    fn run_foreground(&mut self, command: &ParsedCommand) -> anyhow::Result<()> {
        let Some(argument) = command.argv().get(1) else {
            println!("fg command requires PID or %jobid argument");
            return Ok(());
        };

        let Some(job) = self.find_job("fg", argument) else {
            return Ok(());
        };

        nix::sys::signal::killpg(job.pgid, Signal::SIGCONT)?;
        job.state = JobState::Foreground;

        self.wait_foreground()
    }
}
