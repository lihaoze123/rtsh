use std::os::fd::{AsFd, BorrowedFd};

use nix::sys::{
    signal::{SigSet, Signal},
    signalfd::{SfdFlags, SignalFd},
};

pub struct SignalSource {
    fd: SignalFd,
    mask: SigSet,
}

impl SignalSource {
    pub fn new() -> nix::Result<Self> {
        let mut mask = SigSet::empty();
        mask.add(Signal::SIGCHLD);
        mask.add(Signal::SIGINT);
        mask.add(Signal::SIGTSTP);
        mask.add(Signal::SIGQUIT);

        mask.thread_block()?;

        let fd = SignalFd::with_flags(&mask, SfdFlags::SFD_CLOEXEC | SfdFlags::SFD_NONBLOCK)?;
        Ok(Self { fd, mask })
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    pub fn next_signal(&self) -> nix::Result<Option<Signal>> {
        let Some(info) = self.fd.read_signal()? else {
            return Ok(None);
        };
        Ok(Signal::try_from(info.ssi_signo as i32).ok())
    }

    pub fn unblock_in_child(&self) -> nix::Result<()> {
        self.mask.thread_unblock()
    }
}
