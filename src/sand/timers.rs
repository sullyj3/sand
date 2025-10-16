
use std::time::Instant;

use dashmap::{DashMap, Entry};

use crate::sand::timer::*;

#[derive(Default, Debug)]
pub struct Timers(DashMap<TimerId, Timer>);

impl Timers{
    pub fn add(&self, id: TimerId, timer: Timer) {
        if let Some(t) = self.0.insert(id, timer) {
            unreachable!("BUG: adding timer with id #{id:?} clobbered pre-existing timer {t:?}");
        }
    }

    pub fn entry(&self, id: TimerId) -> Entry<'_, TimerId, Timer> {
        self.0.entry(id)
    }

    pub fn get_timerinfo_for_client(&self, now: Instant) -> Vec<TimerInfoForClient> {
        self.0.iter().map(|ref_multi| {
            let (id, timer) = ref_multi.pair();
            TimerInfoForClient::new(*id, timer, now)
        }).collect()
    }
    
    pub(crate) fn elapse(&self, id: TimerId) {
        let Entry::Occupied(occ) = self.0.entry(id) else {
            unreachable!("BUG: tried to complete nonexistent timer #{id:?}");
        };
        occ.remove();
    }

    pub fn minimum_available_id(&self) -> TimerId {
        let mut i = 1;
        while self.0.contains_key(&TimerId(i)) {
            i += 1;
        }
        TimerId(i)
    }
}