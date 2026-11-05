use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputState {
    Ready,
    Recovering,
    Failed,
}

/// Retries are shared across short-lived replacements, so a crash loop cannot
/// reset its own budget. User pause is deliberately owned by the controller.
pub(crate) struct InputRecovery {
    pub state: InputState,
    pub attempts: u8,
    retry_at: Option<Instant>,
    healthy_since: Option<Instant>,
}
impl InputRecovery {
    pub fn new() -> Self {
        Self {
            state: InputState::Ready,
            attempts: 0,
            retry_at: None,
            healthy_since: None,
        }
    }
    pub fn fault(&mut self, now: Instant) {
        if self.state != InputState::Ready {
            return;
        }
        self.healthy(now);
        self.healthy_since = None;
        self.schedule(now);
    }
    fn schedule(&mut self, now: Instant) {
        if self.attempts >= 3 {
            self.state = InputState::Failed;
            self.retry_at = None;
        } else {
            self.state = InputState::Recovering;
            self.retry_at =
                Some(now + Duration::from_millis([500, 1000, 2000][self.attempts as usize]));
        }
    }
    pub fn due(&self, now: Instant) -> bool {
        self.state == InputState::Recovering && self.retry_at.is_some_and(|at| now >= at)
    }
    pub fn attempt(&mut self) {
        self.attempts += 1;
        self.retry_at = None;
    }
    pub fn succeeded(&mut self, now: Instant) {
        self.state = InputState::Ready;
        self.retry_at = None;
        self.healthy_since = Some(now);
    }
    pub fn failed(&mut self, now: Instant) {
        self.schedule(now);
    }
    pub fn healthy(&mut self, now: Instant) {
        if self.state == InputState::Ready
            && self
                .healthy_since
                .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_secs(30))
        {
            self.attempts = 0;
            self.healthy_since = None;
        }
    }
}
