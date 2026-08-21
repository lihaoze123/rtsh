use std::os::fd::{AsFd, BorrowedFd};

use nix::sys::{
    signal::{SigSet, Signal},
    signalfd::{SfdFlags, SignalFd},
};

pub struct SignalSource {
    fd: SignalFd,
    original_mask: SigSet,
}

impl SignalSource {
    pub fn new() -> nix::Result<Self> {
        let mut mask = SigSet::empty();
        mask.add(Signal::SIGCHLD);
        mask.add(Signal::SIGINT);
        mask.add(Signal::SIGTSTP);
        mask.add(Signal::SIGQUIT);

        let original_mask = mask.thread_swap_mask(nix::sys::signal::SigmaskHow::SIG_BLOCK)?;
        let fd = SignalFd::with_flags(&mask, SfdFlags::SFD_CLOEXEC | SfdFlags::SFD_NONBLOCK)?;
        Ok(Self { fd, original_mask })
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    pub fn next_signal(&self) -> nix::Result<Option<Signal>> {
        self.fd
            .read_signal()?
            .map(|info| Signal::try_from(info.ssi_signo as i32))
            .transpose()
    }

    pub fn restore_mask_in_child(&self) -> nix::Result<()> {
        self.original_mask.thread_set_mask()
    }
}
