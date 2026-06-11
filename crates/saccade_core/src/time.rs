use std::time::Instant;

/// Nanoseconds since process-start monotonic epoch.
pub type Ns = u64;

#[derive(Debug, Clone)]
pub struct Clock(Instant);

impl Clock {
    pub fn start() -> Self {
        Self(Instant::now())
    }

    pub fn now_ns(&self) -> Ns {
        self.0.elapsed().as_nanos() as u64
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::start()
    }
}
