use std::{fmt::Display, time::{Duration, Instant}};

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;


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
pub enum TimerStateForClient {
    Paused,
    Running,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TimerInfoForClient {
    id: TimerId,
    state: TimerStateForClient,
    remaining_millis: u64,
}

impl TimerInfoForClient  {
    
    pub fn new(id: TimerId, timer: &Timer, now: Instant) -> Self {
        let (state, remaining_millis) = match timer {
            Timer::Paused { remaining } =>
                (TimerStateForClient::Paused, remaining.as_millis() as u64),
            Timer::Running { due, .. } => 
                (TimerStateForClient::Running, (*due - now).as_millis() as u64),
        };
        Self { id, state, remaining_millis }
    }
}