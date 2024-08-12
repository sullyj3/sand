use serde::{Deserialize, Serialize};
use derive_more::From;

use crate::sand::timer::*;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Command {
    List,
    AddTimer { duration: u64 },
    PauseTimer(TimerId),
    ResumeTimer(TimerId),
    CancelTimer(TimerId),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListResponse {
    Ok { timers: Vec<TimerInfoForClient> },
}
impl ListResponse {
    pub fn ok(timers: Vec<TimerInfoForClient>) -> Self {
        Self::Ok { timers }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddTimerResponse {
    Ok { id: TimerId },
}
impl AddTimerResponse {
    pub fn ok(id: TimerId) -> AddTimerResponse {
        Self::Ok { id }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CancelTimerResponse {
    Ok,
    TimerNotFound,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PauseTimerResponse {
    Ok,
    TimerNotFound,
    AlreadyPaused,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResumeTimerResponse {
    Ok,
    TimerNotFound,
    AlreadyRunning,
}

#[derive(Serialize, Deserialize, From)]
#[serde(untagged)]
pub enum Response {
    List(ListResponse),
    AddTimer(AddTimerResponse),
    CancelTimer(CancelTimerResponse),
    PauseTimer(PauseTimerResponse),
    ResumeTimer(ResumeTimerResponse),

    #[from(ignore)]
    Error(String),
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