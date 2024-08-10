use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use notify_rust::Notification;
use rodio::OutputStreamHandle;
use tokio::sync::Notify;

use crate::sand::audio::ElapsedSoundPlayer;
use crate::sand::timer::Timer;
use crate::sand::timer::TimerId;
use crate::sand::timer::TimerInfoForClient;
use crate::sand::timers::Timers;

#[derive(Clone)]
pub struct DaemonCtx {
    next_id: Arc<Mutex<TimerId>>,
    timers: Arc<Timers>,
    player: Option<ElapsedSoundPlayer>,
}

impl DaemonCtx {
    pub fn new(stream_handle: Option<OutputStreamHandle>) -> Self {
        let player = stream_handle.and_then(|handle| ElapsedSoundPlayer::new(handle).ok());
        Self {
            timers: Default::default(),
            next_id: Arc::new(Mutex::new(Default::default())),
            player,
        }
    }

    pub fn new_timer_id(&self) -> TimerId {
        let mut curr = self.next_id.lock().expect("another thread panicked while holding this lock.");
        let id = *curr;
        *curr = curr.next();
        id
    }

    pub fn get_timerinfo_for_client(&self, now: Instant) -> Vec<TimerInfoForClient> {
        self.timers.get_timerinfo_for_client(now)
    }

    async fn countdown(self, id: TimerId, duration: Duration, rx_added: Arc<Notify>) {
        tokio::time::sleep(duration).await;
        eprintln!("Timer {id} completed");

        let notification = Notification::new()
            .summary("Time's up!")
            .body("Your timer has elapsed")
            .icon("alarm")
            .urgency(notify_rust::Urgency::Critical)
            .show();
        if let Err(e) = notification {
            eprintln!("Error showing desktop notification: {e}");
        }
            
        if let Some(ref player) = self.player {
            eprintln!("playing sound");
            if let Err(e) = player.play() {
                eprintln!("Error playing timer elapsed sound: {e}");
            }
        } else {
            eprintln!("not playing sound");
        }
        rx_added.notified().await;
        self.timers.elapse(id)
    }

    pub fn add_timer(&self, now: Instant, duration: Duration) -> TimerId {
        let id = self.new_timer_id();
        let due = now + duration;

        // once the countdown has elapsed, it removes its associated timer from
        // the Timers map. For short durations (eg 0), We need to synchronize to
        // ensure it doesn't do this til after it's been added
        let notify_added = Arc::new(Notify::new());
        let rx_added = notify_added.clone();
        let countdown = tokio::spawn(
            self.clone().countdown(id, duration, rx_added)
        );
        self.timers.add(id, Timer::Running { due, countdown });
        notify_added.notify_one();
        id
    }
}