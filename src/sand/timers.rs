use std::time::{Duration, Instant};

use dashmap::{DashMap, Entry, VacantEntry};
use indoc::indoc;

use crate::sand::{message::TimerInfo, timer::*};

// TODO: This doesn't really need to be a hashmap, since it's keyed by a newtype
// on a u64. Should be some kind of vec probably. Slotmap doesn't work on its
// own, because the timer ids can't be opaque - they're part of the UI.
// eg `sand pause 1`
#[derive(Default, Debug)]
pub struct Timers(DashMap<TimerId, Timer>);

impl Timers {
    // TODO should remove this and expose a more restrictive interface
    // maybe even pause/resume/cancel functions. Probably a lot of the logic in
    // ctx.rs should be in here
    pub fn entry(&self, id: TimerId) -> Entry<'_, TimerId, Timer> {
        self.0.entry(id)
    }

    pub fn restart(&self, id: TimerId) {
        if let Some(mut timer) = self.0.get_mut(&id) {
            timer.state = TimerState::Running(RunningTimer {
                due: Instant::now() + timer.initial_duration,
            });
        }
    }

    /// Should only be called on running timers
    pub fn set_elapsed(&self, timer_id: TimerId) {
        match self.0.entry(timer_id) {
            Entry::Occupied(mut entry) => {
                let timer = entry.get_mut();
                match &timer.state {
                    TimerState::Running(RunningTimer { due: _, .. }) => {
                        timer.state = TimerState::Elapsed;
                    }
                    t => log::error!(
                        indoc! {"
                            bug: Timer in unexpected state when set_elapsed: {:?}
                            leaving it alone."},
                        t
                    ),
                }
            }
            Entry::Vacant(_) => log::error!(
                indoc! {"
                    bug: Timer {} that we're setting as elapsed doesn't exist
                    ignoring."},
                timer_id
            ),
        }
    }

    pub fn remove(&self, id: TimerId) {
        log::debug!("Removing timer {id}");
        self.0.remove(&id);
    }

    pub fn next_due_running(&self) -> Option<(TimerId, Duration)> {
        let now = Instant::now();
        self.0
            .iter()
            .filter_map(|ref_multi| match &ref_multi.value().state {
                TimerState::Running(running) => {
                    let remaining = running.due - now;
                    Some((*ref_multi.key(), remaining))
                }
                _ => None,
            })
            .min_by_key(|&(_, duration)| duration)
    }

    pub fn get_timerinfo_for_client(&self, now: Instant) -> Vec<TimerInfo> {
        self.0
            .iter()
            .map(|ref_multi| {
                let (id, timer) = ref_multi.pair();
                TimerInfo::new(*id, timer, now)
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
            let TimerState::Running(running) = &mut timer.state else {
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
            self.set_elapsed(*timer_id);
        }
        elapsed_while_asleep
    }
}
