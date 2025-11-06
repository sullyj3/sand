use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use logind_zbus::manager::ManagerProxy;
use notify_rust::Notification;
use tokio::sync::Notify;
use tokio::sync::RwLock;
use tokio_stream::Stream;
use tokio_stream::StreamExt;

use crate::daemon::audio::ElapsedSoundPlayer;
use crate::sand::duration::DurationExt;
use crate::sand::message;
use crate::sand::timer::PausedTimer;
use crate::sand::timer::RunningTimer;
use crate::sand::timer::Timer;
use crate::sand::timer::TimerId;
use crate::sand::timer::TimerInfoForClient;
use crate::sand::timer::TimerState;
use crate::sand::timers::Timers;

/// Should be cheap to clone.
#[derive(Clone)]
pub struct DaemonCtx {
    pub timers: Arc<Timers>,
    pub refresh_next_due: Arc<Notify>,
    pub last_started: Arc<RwLock<Option<Duration>>>,
    pub elapsed_sound_player: Option<ElapsedSoundPlayer>,
}

/// Used to pause the time keeping task during suspend
pub enum KeepTimeState {
    Awake,
    Asleep { slept_at: SystemTime },
}

/// A signal from logind indicating that the system is about to suspend or has
/// resumed from suspend.
enum SuspendSignal {
    GoingToSleep,
    WakingUp,
}

async fn dbus_suspend_events() -> zbus::Result<impl Stream<Item = SuspendSignal>> {
    use zbus::Connection;
    let connection = Connection::system().await?;
    let manager = ManagerProxy::new(&connection).await?;

    let stream = manager
        .receive_prepare_for_sleep()
        .await?
        .filter_map(|signal| {
            signal
                .args()
                .inspect_err(|err| {
                    log::error!("Couldn't get args of PrepareForSleep signal: {err}");
                })
                .map(|args| {
                    log::trace!("Received PrepareForSleep(start={})", args.start);
                    if args.start {
                        SuspendSignal::GoingToSleep
                    } else {
                        SuspendSignal::WakingUp
                    }
                })
                .ok()
        });
    Ok(stream)
}

impl DaemonCtx {
    pub fn get_timerinfo_for_client(&self, now: Instant) -> Vec<TimerInfoForClient> {
        self.timers.get_timerinfo_for_client(now)
    }

    /// The main worker task.
    ///
    /// handles:
    /// - counting down timers
    /// - system sleep and wake
    pub async fn keep_time(&self) -> ! {
        let mut state = KeepTimeState::Awake;
        let suspends_stream = dbus_suspend_events().await.unwrap_or_else(|err| {
            log::error!("Unable to receive D-Bus suspend events: {}", err);
            std::process::exit(1);
        });
        tokio::pin!(suspends_stream);

        loop {
            state = match state {
                KeepTimeState::Asleep { slept_at } => {
                    self.handle_asleep_state(&mut suspends_stream, slept_at)
                        .await
                }
                KeepTimeState::Awake => self.handle_awake_state(&mut suspends_stream).await,
            };
        }
    }

    async fn handle_asleep_state<S>(
        &self,
        suspends_stream: &mut S,
        slept_at: SystemTime,
    ) -> KeepTimeState
    where
        S: Stream<Item = SuspendSignal> + Unpin,
    {
        let Some(signal) = suspends_stream.next().await else {
            log::error!("D-Bus suspend event stream closed");
            std::process::exit(1);
        };

        // expect to wake
        let SuspendSignal::WakingUp = signal else {
            log::warn!(
                "Got notification that the system is about to sleep, but we're already sleeping. Ignoring."
            );
            return KeepTimeState::Asleep { slept_at };
        };

        let woke_at = SystemTime::now();
        let sleep_duration = match woke_at.duration_since(slept_at) {
            Ok(dur) => dur,
            Err(err) => {
                log::error!(
                    "When waking, system clock reported having gone backwards in time since sleeping by {:?}. Assuming no time passed",
                    err.duration()
                );
                Duration::ZERO
            }
        };
        log::info!("System just woke up. Slept for {:?}", sleep_duration);

        let elapsed_while_sleeping = self.timers.awaken(sleep_duration);
        for timer_id in elapsed_while_sleeping {
            tokio::spawn({
                let ctx = self.clone();
                async move { ctx.do_notification(timer_id).await }
            });
        }
        KeepTimeState::Awake
    }

    async fn handle_awake_state<S>(&self, suspends_stream: &mut S) -> KeepTimeState
    where
        S: Stream<Item = SuspendSignal> + Unpin,
    {
        let next_due = self.timers.next_due_running();
        let next_countdown = async move {
            match next_due {
                Some((timer_id, duration)) => {
                    tokio::time::sleep(duration).await;
                    Some(timer_id)
                }
                None => None,
            }
        };

        tokio::select! {
            _ = self.refresh_next_due.notified() => KeepTimeState::Awake,
            Some(signal) = suspends_stream.next() =>
                handle_suspend_signal_awake_state(signal),
            Some(timer_id) = next_countdown => {
                self.timers.set_elapsed(timer_id);
                tokio::spawn({
                    let ctx = self.clone();
                    async move {
                        ctx.do_notification(timer_id).await;
                    }
                });
                log::info!("Timer {timer_id} completed");
                KeepTimeState::Awake
            }
        }
    }

    pub async fn do_notification(&self, timer_id: TimerId) {
        let notification = Notification::new()
            .summary("Time's up!")
            .body(&format!("Timer {timer_id} has elapsed"))
            .icon("alarm")
            .urgency(notify_rust::Urgency::Critical)
            .show_async()
            .await;
        let notification_handle = match notification {
            Ok(notification) => notification,
            Err(e) => {
                log::error!("Error showing desktop notification: {e}");
                return;
            }
        };

        if let Some(ref player) = self.elapsed_sound_player {
            log::debug!("playing sound");
            player.play().await;
        } else {
            log::debug!("player is None - not playing sound");
        }

        notification_handle.wait_for_action(|s| match s {
            "__closed" => log::debug!("Notification for timer {timer_id} closed"),
            _ => log::warn!("Unknown action from notification: {s}"),
        });
        self.timers.remove(&timer_id);
    }

    pub async fn start_timer(&self, now: Instant, duration: Duration) -> TimerId {
        let id = self._start_timer(now, duration);
        log::info!(
            "Started timer {} for {}",
            id,
            duration.format_colon_separated()
        );
        {
            log::trace!("Setting ctx.last_started = {duration:?}");
            *self.last_started.write().await = Some(duration);
        }
        id
    }

    /// Helper for start_timer() and again()
    fn _start_timer(&self, now: Instant, duration: Duration) -> TimerId {
        let vacant = self.timers.first_vacant_entry();
        let id = *vacant.key();
        vacant.insert(Timer::new_running(duration, now));
        self.refresh_next_due.notify_one();
        id
    }

    pub fn pause_timer(&self, id: TimerId, now: Instant) -> message::PauseTimerResponse {
        use message::PauseTimerResponse as Resp;

        let dashmap::Entry::Occupied(mut entry) = self.timers.entry(id) else {
            log::error!("Timer {} not found", id);
            return Resp::TimerNotFound;
        };
        let timer = entry.get_mut();

        use TimerState as TS;
        match timer.state {
            TS::Running(RunningTimer { due }) => {
                let remaining = due - now;
                timer.state = TS::Paused(PausedTimer { remaining });
                self.refresh_next_due.notify_one();
                log::info!(
                    "Paused timer {}, {} remaining",
                    id,
                    remaining.format_colon_separated()
                );
                Resp::Ok
            }
            TS::Paused(_) => {
                log::error!("Timer {} is already paused", id);
                Resp::AlreadyPaused
            }
            TS::Elapsed => {
                log::error!("Timer {} is already elapsed", id);
                Resp::AlreadyElapsed
            }
        }
    }

    pub fn resume_timer(&self, id: TimerId, now: Instant) -> message::ResumeTimerResponse {
        use message::ResumeTimerResponse as Resp;

        let dashmap::Entry::Occupied(mut entry) = self.timers.entry(id) else {
            log::error!("Timer {} not found", id);
            return Resp::TimerNotFound;
        };
        let timer = entry.get_mut();

        use TimerState as TS;
        match timer.state {
            TS::Paused(PausedTimer { remaining }) => {
                log::info!(
                    "Resumed timer {}, {} remaining",
                    id,
                    remaining.format_colon_separated()
                );
                timer.state = TS::Running(RunningTimer {
                    due: now + remaining,
                });
                self.refresh_next_due.notify_one();
                Resp::Ok
            }
            TS::Running(_) => {
                log::error!("Timer {} is already running", id);
                Resp::AlreadyRunning
            }
            TS::Elapsed => {
                log::error!("Timer {} is already elapsed", id);
                Resp::AlreadyElapsed
            }
        }
    }

    pub fn cancel_timer(&self, id: TimerId, now: Instant) -> message::CancelTimerResponse {
        use message::CancelTimerResponse as Resp;

        let dashmap::Entry::Occupied(entry) = self.timers.entry(id) else {
            log::error!("Timer {} not found", id);
            return Resp::TimerNotFound;
        };
        let timer = entry.get();
        match timer.state {
            TimerState::Paused(PausedTimer { remaining }) => {
                log::info!(
                    "Cancelled paused timer {} with {} remaining",
                    id,
                    remaining.format_colon_separated()
                );
            }
            TimerState::Running(RunningTimer { due }) => {
                let remaining = due - now;
                log::info!(
                    "Cancelled running timer {} with {} remaining",
                    id,
                    remaining.format_colon_separated()
                );
            }
            TimerState::Elapsed => {
                log::error!("Timer {} is already elapsed", id);
                return Resp::AlreadyElapsed;
            }
        }
        entry.remove();
        self.refresh_next_due.notify_one();
        Resp::Ok
    }

    pub async fn again(&self, now: Instant) -> message::AgainResponse {
        use message::AgainResponse as Resp;
        let last_started = { *self.last_started.read().await };
        match last_started {
            Some(duration) => {
                let id = self._start_timer(now, duration);
                log::info!(
                    "Restarted most recent timer duration {} with new id {}",
                    duration.format_colon_separated(),
                    id,
                );
                Resp::Ok {
                    id,
                    duration: duration.as_millis() as u64,
                }
            }
            None => Resp::NonePreviouslyStarted,
        }
    }
}

// Whenever the system goes to sleep we
// - stop counting down
// - track how long we've been asleep
// - on wake, elapse and notify those that should have elapsed while asleep
// - resume counting down
fn handle_suspend_signal_awake_state(signal: SuspendSignal) -> KeepTimeState {
    // expect to sleep
    match signal {
        SuspendSignal::WakingUp => {
            log::warn!(
                "Got notification that the system is waking up, but we're already awake. Ignoring."
            );
            KeepTimeState::Awake
        }
        SuspendSignal::GoingToSleep => {
            log::info!("System is preparing to sleep.");
            KeepTimeState::Asleep {
                slept_at: SystemTime::now(),
            }
        }
    }
}
