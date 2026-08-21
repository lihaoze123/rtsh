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
    unistd::{ForkResult, Pid, execvp, fork, setpgid},
};

use crate::{
    jobs::{JobState, JobTable},
    parser::{CommandKind, ParseCommandError, ParsedCommand},
    signal::SignalSource,
};
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
            CommandKind::Jobs | CommandKind::Bg | CommandKind::Fg => {
                println!("builtin not implemented yet");
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

        loop {
            if emit_prompt {
                print!("tsh> ");
                io::stdout().flush()?;
            }

            let line = loop {
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
                    let mut line = String::new();
                    if stdin.read_line(&mut line)? == 0 {
                        return Ok(());
                    }
                    break line;
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
}
