use nix::unistd::Pid;

const MAX_JOBS: usize = 128;

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
        let removed = self.jobs.remove(index);

        self.next_jid = self.jobs.iter().map(|job| job.jid).max().unwrap_or(0) + 1;
        if self.next_jid > MAX_JOBS as u32 {
            self.next_jid = 1;
        }
        Some(removed)
    }

    pub fn set_state(&mut self, pid: Pid, state: JobState) -> Option<&Job> {
        let job = self.jobs.iter_mut().find(|job| job.pid == pid)?;
        job.state = state;
        Some(job)
    }

    pub fn add(
        &mut self,
        pid: Pid,
        pgid: Pid,
        state: JobState,
        cli: String,
    ) -> anyhow::Result<&Job> {
        anyhow::ensure!(self.jobs.len() < MAX_JOBS);

        let jid = self.allocate_jid()?;
        self.jobs.push(Job {
            jid,
            pid,
            pgid,
            state,
            cli,
        });

        Ok(self.jobs.last().unwrap())
    }

    fn allocate_jid(&mut self) -> anyhow::Result<u32> {
        for _ in 0..MAX_JOBS {
            let jid = self.next_jid;
            self.next_jid += 1;
            if self.next_jid > MAX_JOBS as u32 {
                self.next_jid = 1;
            }
            if self.jobs.iter().all(|job| job.jid != jid) {
                return Ok(jid);
            }
        }
        anyhow::bail!("no available jid")
    }
}
