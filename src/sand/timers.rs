
use std::sync::Arc;

use dashmap::DashMap;

use crate::sand::timer::*;

#[derive(Default, Debug, Clone)]
pub struct Timers(Arc<DashMap<TimerId, Timer>>);

impl Timers{
    fn add(&self, id: TimerId, timer: Timer) {
        if let Some(t) = self.0.insert(id, timer) {
            unreachable!("BUG: adding timer with id #{id:?} clobbered pre-existing timer {t:?}");
        }
    }
}