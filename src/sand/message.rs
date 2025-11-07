use std::{
    cmp::Ordering,
    time::{Duration, Instant},
};

use derive_more::From;
use serde::{Deserialize, Serialize};

use crate::sand::timer::*;

/////////////////////////////////////////////////////////////////////////////////////////
// Commands
/////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Command {
    List,
    StartTimer { duration: Duration },
    PauseTimer(TimerId),
    ResumeTimer(TimerId),
    CancelTimer(TimerId),
    Again,
}

/////////////////////////////////////////////////////////////////////////////////////////
// Command responses
/////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListResponse {
    Ok { timers: Vec<TimerInfo> },
}
impl ListResponse {
    pub fn ok(timers: Vec<TimerInfo>) -> Self {
        Self::Ok { timers }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StartTimerResponse {
    Ok { id: TimerId },
}
impl StartTimerResponse {
    pub fn ok(id: TimerId) -> StartTimerResponse {
        Self::Ok { id }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CancelTimerResponse {
    Ok,
    TimerNotFound,
    AlreadyElapsed,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PauseTimerResponse {
    Ok,
    TimerNotFound,
    AlreadyPaused,
    AlreadyElapsed,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResumeTimerResponse {
    Ok,
    TimerNotFound,
    AlreadyRunning,
    AlreadyElapsed,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgainResponse {
    Ok { id: TimerId, duration: u64 },
    NonePreviouslyStarted,
}

#[derive(Serialize, Deserialize, From)]
#[serde(untagged)]
pub enum Response {
    List(ListResponse),
    AddTimer(StartTimerResponse),
    CancelTimer(CancelTimerResponse),
    PauseTimer(PauseTimerResponse),
    ResumeTimer(ResumeTimerResponse),
    Again(AgainResponse),

    #[from(ignore)]
    Error(String),
}

/////////////////////////////////////////////////////////////////////////////////////////
// Timers
/////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum TimerStateClient {
    Paused,
    Running,
    Elapsed,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TimerInfo {
    pub id: TimerId,
    pub state: TimerStateClient,
    pub remaining: Duration,
}

impl TimerInfo {
    pub fn new(id: TimerId, timer: &Timer, now: Instant) -> Self {
        let (state, remaining) = match timer.state {
            TimerState::Paused(PausedTimer { remaining }) => (TimerStateClient::Paused, remaining),
            TimerState::Running(RunningTimer { due, .. }) => {
                (TimerStateClient::Running, (due - now))
            }
            // TODO would be better to have a negative duration for this case
            TimerState::Elapsed => (TimerStateClient::Elapsed, Duration::ZERO),
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn serde_message() {
        let cmd = Command::List;
        let serialized = serde_json::to_string(&cmd).unwrap();
        assert_eq!("\"list\"", serialized);

        let deserialized: Command = serde_json::from_str(&serialized).unwrap();
        assert_eq!(Command::List, deserialized);
    }

    #[test]
    fn serde_list_response() {
        let response = ListResponse::ok(vec![]);
        let serialized = serde_json::to_string(&response).unwrap();
        assert_eq!("{\"ok\":{\"timers\":[]}}", serialized);

        let deserialized: ListResponse = serde_json::from_str(&serialized).unwrap();
        assert_eq!(ListResponse::ok(vec![]), deserialized);
    }
}
