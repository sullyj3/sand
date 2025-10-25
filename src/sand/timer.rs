use std::{
    cmp::Ordering,
    fmt::Display,
    time::{Duration, Instant},
};

use derive_more::FromStr;
use serde::{Deserialize, Serialize};

use crate::sand::duration::DurationExt;

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
    pub remaining_millis: u64,
}

impl TimerInfoForClient {
    pub fn new(id: TimerId, timer: &Timer, now: Instant) -> Self {
        let (state, remaining_millis) = match timer {
            Timer::Paused(PausedTimer { remaining }) => {
                (TimerState::Paused, remaining.as_millis() as u64)
            }
            Timer::Running(RunningTimer { due, .. }) => {
                (TimerState::Running, (*due - now).as_millis() as u64)
            }
        };
        Self {
            id,
            state,
            remaining_millis,
        }
    }

    pub fn display(&self, first_column_width: usize) -> String {
        let remaining: String =
            Duration::from_millis(self.remaining_millis).format_colon_separated();
        let id = self.id;
        let play_pause = match self.state {
            TimerState::Paused => " ⏸ ",
            TimerState::Running => " ▶ ",
        };
        format!(
            "{play_pause} │ {:>width$} │ {remaining}",
            id.to_string(),
            width = first_column_width
        )
    }

    pub fn cmp_by_next_due(t1: &Self, t2: &Self) -> Ordering {
        t1.remaining_millis.cmp(&t2.remaining_millis)
    }
}
