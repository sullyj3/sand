use std::{
    fmt::Display,
    time::{Duration, Instant},
};

use derive_more::FromStr;
use serde::{Deserialize, Serialize};

#[derive(
    PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy, Serialize, Deserialize, FromStr,
)]
pub struct TimerId(pub u64);

impl Default for TimerId {
    fn default() -> Self {
        Self(1)
    }
}

impl Display for TimerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[derive(Debug)]
pub struct PausedTimer {
    pub remaining: Duration,
}

#[derive(Debug)]
pub struct RunningTimer {
    pub due: Instant,
}

// TODO some of this is daemon-specific and should maybe go in
// a daemon/timer.rs module
#[derive(Debug)]
pub struct Timer {
    /// The initial duration of the timer. Should not be modified after creation.
    pub initial_duration: Duration,
    pub state: TimerState,
}

impl Timer {
    pub fn new_running(now: Instant, initial_duration: Duration) -> Self {
        Timer {
            initial_duration,
            state: TimerState::Running(RunningTimer {
                due: now + initial_duration,
            }),
        }
    }
}

// TODO some of this is daemon-specific and should maybe go in
// a daemon/timer.rs module
#[derive(Debug)]
pub enum TimerState {
    Paused(PausedTimer),
    Running(RunningTimer),
    /// We keep timers after they've elapsed in this state to reserve the timer ID,
    /// allowing the user to restart them from the notification with the same ID.
    Elapsed,
}
