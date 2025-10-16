use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use notify_rust::Notification;
use rodio::OutputStreamHandle;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::sand::audio::ElapsedSoundPlayer;
use crate::sand::message::PauseTimerResponse;
use crate::sand::message;
use crate::sand::timer::Timer;
use crate::sand::timer::TimerId;
use crate::sand::timer::TimerInfoForClient;
use crate::sand::timers::Timers;

#[derive(Clone)]
pub struct DaemonCtx {
    timers: Arc<Timers>,
    player: Option<ElapsedSoundPlayer>,
}

impl DaemonCtx {
    pub fn new(stream_handle: Option<OutputStreamHandle>) -> Self {
        log::trace!("stream_handle is {}", if stream_handle.is_some() {"some"} else {"none"});
        let player = stream_handle.and_then(|handle| {
            let elapsed_sound_player = ElapsedSoundPlayer::new(handle);
            log::trace!(
                "elapsed_sound_player is {}",
                if elapsed_sound_player.is_ok() {"ok"} else {"err"});
            if let Err(e) = &elapsed_sound_player {
                log::debug!("{:?}", e);
            }
            elapsed_sound_player.ok()
        });
        log::trace!("player is {}", if player.is_some() {"some"} else {"none"});
        match player {
            Some(_) => log::debug!("ElapsedSoundPlayer successfully initialized."),
            None => log::warn!(
                "Failed to initialize elapsed sound player.\n\
                 There will be no timer sounds."),
        }
        Self {
            timers: Default::default(),
            player,
        }
    }

    pub fn get_timerinfo_for_client(&self, now: Instant) -> Vec<TimerInfoForClient> {
        self.timers.get_timerinfo_for_client(now)
    }

    async fn countdown(self, id: TimerId, duration: Duration, rx_added: Arc<Notify>) {
        tokio::time::sleep(duration).await;
        log::info!("Timer {id} completed");

        let notification = Notification::new()
            .summary("Time's up!")
            .body("Your timer has elapsed")
            .icon("alarm")
            .urgency(notify_rust::Urgency::Critical)
            .show();
        if let Err(e) = notification {
            log::error!("Error showing desktop notification: {e}");
        }
            
        if let Some(ref player) = self.player {
            log::debug!("playing sound");
            if let Err(e) = player.play() {
                log::error!("Error playing timer elapsed sound: {e}");
            }
        } else {
            log::debug!("DaemonCtx.play is None - not playing sound");
        }
        rx_added.notified().await;
        self.timers.elapse(id)
    }

    fn spawn_countdown(&self, id: TimerId, duration: Duration) -> (JoinHandle<()>, Arc<Notify>)  {
        // once the countdown has elapsed, it removes its associated timer from
        // the Timers map. For short durations (eg 0), We need to synchronize to
        // ensure it doesn't do this til after it's been added
        let notify_added = Arc::new(Notify::new());
        let rx_added = notify_added.clone();
        let join_handle = tokio::spawn(
            self.clone().countdown(id, duration, rx_added)
        );
        (join_handle, notify_added)
    }

    pub fn add_timer(&self, now: Instant, duration: Duration) -> TimerId {
        let vacant = self.timers.first_vacant_entry();
        let id = *vacant.key();

        let (join_handle, notify_added) = self.spawn_countdown(id, duration);
        vacant.insert(Timer::Running { due: now + duration, countdown: join_handle });
        notify_added.notify_one();
        id
    }

    pub fn pause_timer(&self, id: TimerId, now: Instant) -> PauseTimerResponse {
        use PauseTimerResponse as Resp;
        use Timer as T;
        
        let dashmap::Entry::Occupied(mut entry) = self.timers.entry(id) else {
            return Resp::TimerNotFound;
        };
        let timer = entry.get_mut();
        let T::Running { due, countdown } = timer else {
            return Resp::AlreadyPaused
        };

        countdown.abort();
        *timer = T::Paused { remaining: *due - now };
        Resp::Ok
    }
    
    pub fn resume_timer(&self, id: TimerId, now: Instant) -> message::ResumeTimerResponse {
        use message::ResumeTimerResponse as Resp;
        use Timer as T;
        
        let dashmap::Entry::Occupied(mut entry) = self.timers.entry(id) else {
            return Resp::TimerNotFound;
        };
        let timer = entry.get_mut();
        let T::Paused { remaining } = timer else {
            return Resp::AlreadyRunning
        };

        let (join_handle, notify_added) = self.spawn_countdown(id, *remaining);
        *timer = T::Running { due: now + *remaining, countdown: join_handle };
        notify_added.notify_one();
        Resp::Ok
    }
    
    pub fn cancel_timer(&self, id: TimerId) -> message::CancelTimerResponse {
        use message::CancelTimerResponse as Resp;

        let dashmap::Entry::Occupied(entry) = self.timers.entry(id) else {
            return Resp::TimerNotFound;
        };
        let timer = entry.get();
        if let Timer::Running { countdown, .. } = timer {
            countdown.abort();
        }
        entry.remove();
        Resp::Ok
    }
}
