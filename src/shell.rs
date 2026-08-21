use std::{
    io::{self, Write},
    os::fd::AsFd,
    println,
};

use nix::{
    errno::Errno,
    poll::{PollFd, PollFlags, PollTimeout, poll},
    sys::{
        signal::Signal,
        wait::{WaitPidFlag, WaitStatus, waitpid},
    },
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
            CommandKind::Jobs | CommandKind::Bg | CommandKind::Fg | CommandKind::External => {
                println!("implement command execution next");
                Ok(true)
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
}
