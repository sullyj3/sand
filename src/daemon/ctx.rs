use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use logind_zbus::manager::ManagerProxy;
use tokio::sync::mpsc::Sender;
use tokio::sync::Notify;
use tokio_stream::Stream;
use tokio_stream::StreamExt;

use crate::daemon::ElapsedEvent;
use crate::sand::duration::DurationExt;
use crate::sand::message;
use crate::sand::timer::PausedTimer;
use crate::sand::timer::RunningTimer;
use crate::sand::timer::Timer;
use crate::sand::timer::TimerId;
use crate::sand::timer::TimerInfoForClient;
use crate::sand::timers::Timers;

/// Should be cheap to clone.
#[derive(Clone)]
pub struct DaemonCtx {
    pub timers: Arc<Timers>,
    pub tx_elapsed_events: Sender<ElapsedEvent>,
    pub refresh_next_due: Arc<Notify>,
}

// Used to pause the time keeping task during suspend
pub enum KeepTimeState {
    Awake,
    Sleeping { slept_at: SystemTime },
}

enum SuspendSignal {
    Sleeping,
    Waking,
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
                        SuspendSignal::Sleeping
                    } else {
                        SuspendSignal::Waking
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
                KeepTimeState::Sleeping { slept_at } => {
                    self.handle_sleeping_state(&mut suspends_stream, slept_at)
                        .await
                }
                KeepTimeState::Awake => self.handle_awake_state(&mut suspends_stream).await,
            };
        }
    }

    async fn handle_sleeping_state<S>(
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
        let SuspendSignal::Waking = signal else {
            log::warn!(
                "Got notification that the system is about to sleep, but we're already sleeping. Ignoring."
            );
            return KeepTimeState::Sleeping { slept_at };
        };

        let woke_at = SystemTime::now();
        let sleep_duration = match woke_at.duration_since(slept_at) {
            Ok(dur) => dur,
            Err(err) => {
                log::error!("When waking, system clock reported having gone backwards in time since sleeping by {:?}. Assuming no time passed", err.duration());
                Duration::ZERO
            }
        };
        log::info!("System just woke up. Slept for {:?}", sleep_duration);

        let elapsed_while_sleeping = self.timers.awaken(sleep_duration);
        for timer_id in elapsed_while_sleeping {
            self.tx_elapsed_events
                .send(ElapsedEvent(timer_id))
                .await
                .expect("elapsed event receiver was closed");
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
                self.tx_elapsed_events
                    .send(ElapsedEvent(timer_id))
                    .await
                    .expect("elapsed event receiver was closed");
                log::info!("Timer {timer_id} completed");
                self.timers.remove(&timer_id);
                KeepTimeState::Awake
            }
        }
    }

    pub fn add_timer(&self, now: Instant, duration: Duration) -> TimerId {
        let vacant = self.timers.first_vacant_entry();
        let id = *vacant.key();

        vacant.insert(Timer::Running(RunningTimer {
            due: now + duration,
        }));
        self.refresh_next_due.notify_one();
        log::info!(
            "Started timer {} for {}",
            id,
            duration.format_colon_separated()
        );
        id
    }

    pub fn pause_timer(&self, id: TimerId, now: Instant) -> message::PauseTimerResponse {
        use message::PauseTimerResponse as Resp;

        let dashmap::Entry::Occupied(mut entry) = self.timers.entry(id) else {
            log::error!("Timer {} not found", id);
            return Resp::TimerNotFound;
        };
        let timer = entry.get_mut();

        use Timer as T;
        match timer {
            T::Running(RunningTimer { due }) => {
                let remaining = *due - now;
                *timer = T::Paused(PausedTimer { remaining });
                self.refresh_next_due.notify_one();
                log::info!(
                    "Paused timer {}, {} remaining",
                    id,
                    remaining.format_colon_separated()
                );
                Resp::Ok
            }
            T::Paused(_) => {
                log::error!("Timer {} is already paused", id);
                Resp::AlreadyPaused
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

        use Timer as T;
        match timer {
            T::Paused(PausedTimer { remaining }) => {
                log::info!(
                    "Resumed timer {}, {} remaining",
                    id,
                    remaining.format_colon_separated()
                );
                *timer = T::Running(RunningTimer {
                    due: now + *remaining,
                });
                self.refresh_next_due.notify_one();
                Resp::Ok
            }
            T::Running(_) => {
                log::error!("Timer {} is already running", id);
                Resp::AlreadyRunning
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
        match timer {
            Timer::Paused(PausedTimer { remaining }) => {
                log::info!(
                    "Cancelled paused timer {} with {} remaining",
                    id,
                    remaining.format_colon_separated()
                );
            }
            Timer::Running(RunningTimer { due }) => {
                let remaining = *due - now;
                log::info!(
                    "Cancelled running timer {} with {} remaining",
                    id,
                    remaining.format_colon_separated()
                );
            }
        }
        entry.remove();
        self.refresh_next_due.notify_one();
        Resp::Ok
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
        SuspendSignal::Waking => {
            log::warn!(
                "Got notification that the system is waking up, but we're already awake. Ignoring."
            );
            KeepTimeState::Awake
        }
        SuspendSignal::Sleeping => {
            log::info!("System is preparing to sleep.");
            KeepTimeState::Sleeping {
                slept_at: SystemTime::now(),
            }
        }
    }
}
