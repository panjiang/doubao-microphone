//! Pure, clock-driven state machine. No hardware, network or wall clock access.
use anyhow::{Result, bail, ensure};
use serde::Serialize;
use std::time::Duration;
use uuid::Uuid;

pub const LEASE: Duration = Duration::from_millis(1000);
pub const TAIL: Duration = Duration::from_millis(150);
pub const END_LIMIT: Duration = Duration::from_millis(800);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Disconnected,
    Idle,
    Priming,
    Streaming,
    Draining,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    Ready(Uuid),
    Ended {
        id: Uuid,
        complete: bool,
        reason: &'static str,
    },
}

pub struct Session {
    pub phase: Phase,
    pub id: Option<Uuid>,
    pub last_seq: Option<u32>,
    pub heartbeat: Duration,
    pub last_audio: Duration,
    pub warmup: Duration,
    started: Duration,
    ending: Duration,
    quiet_since: Option<Duration>,
    target_inactive_since: Option<Duration>,
    seen: std::collections::HashSet<Uuid>,
}

impl Session {
    pub fn new(warmup: Duration) -> Self {
        Self {
            phase: Phase::Disconnected,
            id: None,
            last_seq: None,
            heartbeat: Duration::ZERO,
            last_audio: Duration::ZERO,
            warmup,
            started: Duration::ZERO,
            ending: Duration::ZERO,
            quiet_since: None,
            target_inactive_since: None,
            seen: std::collections::HashSet::new(),
        }
    }
    pub fn connect(&mut self, now: Duration) {
        self.phase = Phase::Idle;
        self.heartbeat = now;
        self.seen.clear();
    }
    pub fn active(&self) -> bool {
        self.id.is_some()
    }
    pub fn begin(&mut self, id: Uuid, now: Duration) -> Result<bool> {
        if self.id == Some(id) {
            return Ok(false);
        }
        ensure!(self.phase == Phase::Idle, "session not idle");
        ensure!(!self.seen.contains(&id), "session already ended");
        ensure!(self.seen.len() < 4096, "请重新连接手机，开始新的连接周期");
        self.seen.insert(id);
        self.id = Some(id);
        self.last_seq = None;
        self.started = now;
        self.last_audio = now;
        self.quiet_since = None;
        self.phase = Phase::Priming;
        self.target_inactive_since = None;
        Ok(true)
    }
    pub fn shortcut_pressed(&mut self, now: Duration) {
        self.started = now;
        self.last_audio = now;
    }
    /// False means an old session or a duplicate. Missing sequence numbers are fatal.
    pub fn frame(&mut self, id: Uuid, sequence: u32, now: Duration) -> Result<bool> {
        if self.id != Some(id) {
            return Ok(false);
        }
        ensure!(
            matches!(self.phase, Phase::Priming | Phase::Streaming),
            "audio after end"
        );
        let expected = match self.last_seq {
            Some(n) => n
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("sequence exhausted"))?,
            None => 0,
        };
        if sequence < expected {
            return Ok(false);
        }
        ensure!(sequence == expected, "audio sequence gap");
        self.last_seq = Some(sequence);
        self.last_audio = now;
        Ok(true)
    }
    pub fn end(
        &mut self,
        id: Uuid,
        last: Option<u32>,
        now: Duration,
    ) -> Result<Option<Transition>> {
        if self.id != Some(id) || self.phase == Phase::Draining {
            return Ok(None);
        }
        ensure!(last == self.last_seq, "last audio frame was not received");
        if self.phase == Phase::Priming {
            return Ok(self.abort("short_press"));
        }
        ensure!(self.phase == Phase::Streaming, "session not streaming");
        self.phase = Phase::Draining;
        self.ending = now;
        self.quiet_since = None;
        Ok(None)
    }
    pub fn tick(&mut self, now: Duration, queued_ms: u64) -> Option<Transition> {
        if !self.active() {
            return None;
        }
        if now.saturating_sub(self.heartbeat) >= LEASE {
            return self.abort("heartbeat_timeout");
        }
        if queued_ms > 750 {
            return self.abort("audio_backlog");
        }
        if matches!(self.phase, Phase::Priming | Phase::Streaming)
            && now.saturating_sub(self.last_audio) >= LEASE
        {
            return self.abort("audio_timeout");
        }
        match self.phase {
            Phase::Priming => {
                if now.saturating_sub(self.started) > Duration::from_millis(500) {
                    return self.abort("startup_timeout");
                }
                if now.saturating_sub(self.started) >= self.warmup {
                    self.phase = Phase::Streaming;
                    return self.id.map(Transition::Ready);
                }
            }
            Phase::Draining => {
                if now.saturating_sub(self.ending) >= END_LIMIT {
                    return self.abort("drain_timeout");
                }
                if queued_ms == 0 {
                    let start = *self.quiet_since.get_or_insert(now);
                    if now.saturating_sub(start) >= TAIL {
                        return self.finish(true, "completed");
                    }
                } else {
                    self.quiet_since = None;
                }
            }
            _ => {}
        }
        None
    }
    pub fn abort(&mut self, reason: &'static str) -> Option<Transition> {
        self.finish(false, reason)
    }
    /// A single inactive snapshot is insufficient to stop a live session.
    /// Require a later inactive observation; any recovery clears the timer.
    pub fn target_state(&mut self, recording: bool, now: Duration) -> Option<Transition> {
        if !self.active() || recording {
            self.target_inactive_since = None;
            return None;
        }
        let first = *self.target_inactive_since.get_or_insert(now);
        if now.saturating_sub(first) >= Duration::from_millis(150) {
            return self.abort("doubao_stopped");
        }
        None
    }
    fn finish(&mut self, complete: bool, reason: &'static str) -> Option<Transition> {
        let id = self.id.take()?;
        self.phase = Phase::Idle;
        self.quiet_since = None;
        Some(Transition::Ended {
            id,
            complete,
            reason,
        })
    }
    pub fn set_warmup(&mut self, ms: u64) -> Result<()> {
        if self.active() {
            bail!("cannot configure during dictation");
        }
        ensure!((100..=450).contains(&ms), "warmup must be 100..450 ms");
        self.warmup = Duration::from_millis(ms);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }
    fn setup() -> (Session, Uuid) {
        let mut s = Session::new(ms(350));
        s.connect(ms(0));
        let id = Uuid::new_v4();
        s.begin(id, ms(0)).unwrap();
        (s, id)
    }
    #[test]
    fn warmup_and_tail_are_distinct() {
        let (mut s, id) = setup();
        s.frame(id, 0, ms(20)).unwrap();
        assert!(s.tick(ms(349), 350).is_none());
        assert_eq!(s.tick(ms(350), 350), Some(Transition::Ready(id)));
        s.end(id, Some(0), ms(400)).unwrap();
        assert!(s.tick(ms(500), 10).is_none());
        assert!(s.tick(ms(600), 0).is_none());
        assert!(s.tick(ms(749), 0).is_none());
        assert_eq!(
            s.tick(ms(750), 0),
            Some(Transition::Ended {
                id,
                complete: true,
                reason: "completed"
            })
        );
    }
    #[test]
    fn short_click_never_replays() {
        let (mut s, id) = setup();
        let e = s.end(id, None, ms(20)).unwrap().unwrap();
        assert!(matches!(
            e,
            Transition::Ended {
                complete: false,
                reason: "short_press",
                ..
            }
        ));
        assert!(s.tick(ms(350), 0).is_none());
    }
    #[test]
    fn old_duplicate_and_missing_frames() {
        let (mut s, id) = setup();
        assert!(!s.begin(id, ms(1)).unwrap());
        assert!(!s.frame(Uuid::new_v4(), 9, ms(1)).unwrap());
        assert!(s.frame(id, 0, ms(10)).unwrap());
        assert!(!s.frame(id, 0, ms(20)).unwrap());
        assert!(s.frame(id, 2, ms(30)).is_err());
        assert!(s.end(id, Some(1), ms(40)).is_err());
        s.abort("test");
        assert!(!s.frame(id, 1, ms(50)).unwrap());
        assert!(s.begin(id, ms(60)).is_err());
    }
    #[test]
    fn heartbeat_expires_even_with_audio() {
        let (mut s, id) = setup();
        s.frame(id, 0, ms(999)).unwrap();
        assert!(matches!(
            s.tick(ms(1000), 0),
            Some(Transition::Ended {
                reason: "heartbeat_timeout",
                ..
            })
        ));
    }
    #[test]
    fn drain_has_a_hard_limit_and_duplicates_do_not_extend_it() {
        let (mut s, id) = setup();
        s.tick(ms(350), 0);
        s.end(id, None, ms(400)).unwrap();
        s.heartbeat = ms(1100);
        assert!(s.end(id, None, ms(1100)).unwrap().is_none());
        assert!(matches!(
            s.tick(ms(1200), 20),
            Some(Transition::Ended {
                reason: "drain_timeout",
                ..
            })
        ));
    }
    #[test]
    fn audio_backlog_and_audio_stall_abort() {
        let (mut s, _) = setup();
        assert!(matches!(
            s.tick(ms(20), 751),
            Some(Transition::Ended {
                reason: "audio_backlog",
                ..
            })
        ));
        let (mut s, _) = setup();
        s.heartbeat = ms(999);
        assert!(matches!(
            s.tick(ms(1000), 0),
            Some(Transition::Ended {
                reason: "audio_timeout",
                ..
            })
        ));
    }
    #[test]
    fn configuration_is_blocked_during_a_session() {
        let (mut s, _) = setup();
        assert!(s.set_warmup(100).is_err());
        s.abort("test");
        assert!(s.set_warmup(450).is_ok());
        assert!(s.set_warmup(501).is_err());
    }
    #[test]
    fn an_older_completed_id_cannot_be_reused_after_another_session() {
        let (mut s, a) = setup();
        s.abort("test");
        let b = Uuid::new_v4();
        s.begin(b, ms(10)).unwrap();
        s.abort("test");
        assert!(s.begin(a, ms(20)).is_err());
        s.connect(ms(30));
        assert!(s.begin(a, ms(30)).is_ok());
    }
    #[test]
    fn warmup_starts_after_shortcut_ack_not_before_ipc() {
        let (mut s, id) = setup();
        s.shortcut_pressed(ms(200));
        s.frame(id, 0, ms(210)).unwrap();
        assert!(s.tick(ms(350), 350).is_none());
        assert_eq!(s.tick(ms(550), 350), Some(Transition::Ready(id)));
    }
    #[test]
    fn target_inactive_snapshot_can_recover_without_cutting_audio() {
        let (mut s, id) = setup();
        s.tick(ms(350), 350);
        assert!(s.target_state(false, ms(400)).is_none());
        assert!(s.target_state(false, ms(400)).is_none());
        assert!(s.frame(id, 0, ms(450)).unwrap());
        assert!(s.target_state(true, ms(650)).is_none());
        assert!(s.target_state(false, ms(900)).is_none());
        assert!(s.target_state(true, ms(1150)).is_none());
        assert_eq!(s.phase, Phase::Streaming);
    }
    #[test]
    fn confirmed_target_stop_aborts_even_if_phone_keeps_streaming() {
        let (mut s, id) = setup();
        s.tick(ms(350), 350);
        assert!(s.target_state(false, ms(400)).is_none());
        assert!(s.frame(id, 0, ms(450)).unwrap());
        assert!(s.target_state(false, ms(549)).is_none());
        assert!(
            matches!(s.target_state(false, ms(650)), Some(Transition::Ended {
            id: ended_id, reason: "doubao_stopped", complete: false
        }) if ended_id == id)
        );
        assert!(!s.frame(id, 1, ms(700)).unwrap());
        assert!(s.target_state(false, ms(900)).is_none());
    }
}
