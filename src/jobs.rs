use nix::unistd::Pid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Foreground,
    Background,
    Stopped,
}

#[derive(Debug)]
pub struct Job {
    pub jid: u32,
    pub pid: Pid,
    pub pgid: Pid,
    pub state: JobState,
    pub cli: String,
}

pub struct JobTable {
    jobs: Vec<Job>,
    next_jid: u32,
}

impl JobTable {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            next_jid: 1,
        }
    }

    pub fn foreground_pgid(&self) -> Option<Pid> {
        self.jobs
            .iter()
            .find(|job| job.state == JobState::Foreground)
            .map(|job| job.pgid)
    }

    pub fn remove_by_pid(&mut self, pid: Pid) -> Option<Job> {
        let index = self.jobs.iter().position(|job| job.pid == pid)?;
        Some(self.jobs.remove(index))
    }

    pub fn set_state(&mut self, pid: Pid, state: JobState) -> Option<&Job> {
        let job = self.jobs.iter_mut().find(|job| job.pid == pid)?;
        job.state = state;
        Some(job)
    }
}
