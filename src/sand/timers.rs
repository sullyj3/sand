
use std::time::{Duration, Instant};

use dashmap::{DashMap, Entry, VacantEntry};

use crate::sand::timer::*;

#[derive(Default, Debug)]
pub struct Timers(pub DashMap<TimerId, Timer>);

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

    // cancel countdown tasks for all running timers, returning a list of their
    // ids and remaining durations
    pub fn cancel_running_countdowns(&self) -> Vec<(TimerId, Duration)> {
        let mut running_timers = Vec::with_capacity(self.0.len());
        for ref_multi in &self.0 {
            if let Timer::Running {due, countdown} = ref_multi.value() {
                countdown.abort();
                let remaining: Duration = *due - Instant::now();
                running_timers.push((*ref_multi.key(), remaining));
            }
        }
        running_timers
    }
}