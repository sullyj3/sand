
use std::time::Instant;

use dashmap::{DashMap, Entry, VacantEntry};

use crate::sand::timer::*;

#[derive(Default, Debug)]
pub struct Timers(DashMap<TimerId, Timer>);

impl Timers{
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

    pub fn first_vacant_entry(&self) -> VacantEntry<'_, TimerId, Timer> {
        (1..).find_map(|id| {
            match self.0.entry(TimerId(id)) {
                Entry::Occupied(_) => None,
                Entry::Vacant(vacant_entry) => Some(vacant_entry),
            }
        }).unwrap()
    }
}