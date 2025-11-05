use std::time::{Duration, Instant};

use dashmap::{DashMap, Entry, VacantEntry};

use crate::sand::timer::*;

// TODO: This doesn't really need to be a hashmap, since it's keyed by a newtype
// on a u64. Should be some kind of vec probably. Slotmap doesn't work on its
// own, because the timer ids can't be opaque - they're part of the UI.
// eg `sand pause 1`
#[derive(Default, Debug)]
pub struct Timers(DashMap<TimerId, Timer>);

impl Timers {
    pub fn entry(&self, id: TimerId) -> Entry<'_, TimerId, Timer> {
        self.0.entry(id)
    }

    pub fn remove(&self, id: &TimerId) {
        self.0.remove(id);
    }

    pub fn next_due_running(&self) -> Option<(TimerId, Duration)> {
        let now = Instant::now();
        self.0
            .iter()
            .filter_map(|ref_multi| match ref_multi.value() {
                Timer::Running(running) => {
                    let remaining = running.due - now;
                    Some((*ref_multi.key(), remaining))
                }
                _ => None,
            })
            .min_by_key(|&(_, duration)| duration)
    }

    pub fn get_timerinfo_for_client(&self, now: Instant) -> Vec<TimerInfoForClient> {
        self.0
            .iter()
            .map(|ref_multi| {
                let (id, timer) = ref_multi.pair();
                TimerInfoForClient::new(*id, timer, now)
            })
            .collect()
    }

    pub fn first_vacant_entry(&self) -> VacantEntry<'_, TimerId, Timer> {
        (1..)
            .find_map(|id| match self.0.entry(TimerId(id)) {
                Entry::Occupied(_) => None,
                Entry::Vacant(vacant_entry) => Some(vacant_entry),
            })
            .unwrap()
    }

    // remove and return all running timers that should have elapsed while
    // asleep, and deduct the sleep duration from the due time of the remaining
    // running timers
    pub fn awaken(&self, sleep_duration: Duration) -> Vec<TimerId> {
        let mut elapsed_while_asleep = Vec::new();
        let now = Instant::now();
        for mut ref_mut_multi in self.0.iter_mut() {
            let (timer_id, timer) = ref_mut_multi.pair_mut();
            let Timer::Running(running) = timer else {
                continue;
            };

            let before_remaining = running.due - now;
            log::trace!("Before sleep, timer {timer_id} had {before_remaining:?} remaining.");
            running.due -= sleep_duration;
            let after_remaining = running.due - now;
            log::trace!("After sleep, timer {timer_id} has {after_remaining:?} remaining.");
            if running.due <= now {
                elapsed_while_asleep.push(*timer_id);
            }
        }
        for timer_id in &elapsed_while_asleep {
            self.remove(timer_id);
        }
        elapsed_while_asleep
    }
}
