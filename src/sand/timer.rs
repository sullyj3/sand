use std::{fmt::Display, time::{Duration, Instant}};

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;

use crate::sand::duration::DurationExt;


#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Serialize, Deserialize)]
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

impl TimerId {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    // TODO this should return the result and quitting should be in the client
    pub fn parse_or_quit(timer_id: &str) -> Self {
        u64::from_str_radix(&timer_id, 10)
            .map(TimerId)
            .unwrap_or_else(|e| {
                eprintln!("Failed to parse timer id \"{timer_id}\": {e}");
                std::process::exit(1)
            })
    }
}

#[derive(Debug)]
pub enum Timer {
    Paused { remaining: Duration },
    Running { due: Instant, countdown: JoinHandle<()>},
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum TimerState {
    Paused,
    Running,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TimerInfoForClient {
    id: TimerId,
    state: TimerState,
    remaining_millis: u64,
}

impl TimerInfoForClient  {
    
    pub fn new(id: TimerId, timer: &Timer, now: Instant) -> Self {
        let (state, remaining_millis) = match timer {
            Timer::Paused { remaining } =>
                (TimerState::Paused, remaining.as_millis() as u64),
            Timer::Running { due, .. } => 
                (TimerState::Running, (*due - now).as_millis() as u64),
        };
        Self { id, state, remaining_millis }
    }


    pub fn display(&self) -> String {
        let remaining: String = Duration::from_millis(self.remaining_millis)
            .format_colon_separated();
        let id = self.id;
        const PAUSED: &'static str = " (PAUSED)";
        const NOT_PAUSED: &'static str = "";
        let maybe_paused = 
            if self.state == TimerState::Paused { PAUSED } else { NOT_PAUSED };
        format!("{id} | {remaining}{maybe_paused}")
    }
}