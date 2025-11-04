use std::{
    cmp::Ordering,
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

#[derive(Debug)]
pub enum Timer {
    Paused(PausedTimer),
    Running(RunningTimer),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum TimerState {
    Paused,
    Running,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TimerInfoForClient {
    pub id: TimerId,
    pub state: TimerState,
    pub remaining: Duration,
}

impl TimerInfoForClient {
    pub fn new(id: TimerId, timer: &Timer, now: Instant) -> Self {
        let (state, remaining) = match timer {
            Timer::Paused(PausedTimer { remaining }) => (TimerState::Paused, *remaining),
            Timer::Running(RunningTimer { due, .. }) => (TimerState::Running, (*due - now)),
        };
        Self {
            id,
            state,
            remaining,
        }
    }

    pub fn cmp_by_next_due(t1: &Self, t2: &Self) -> Ordering {
        t1.remaining.cmp(&t2.remaining)
    }

    pub fn cmp_by_id(t1: &Self, t2: &Self) -> Ordering {
        t1.id.cmp(&t2.id)
    }
}
