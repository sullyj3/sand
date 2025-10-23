use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use logind_zbus::manager::ManagerProxy;
use logind_zbus::manager::PrepareForSleepStream;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::Notify;
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
    pub tx_keep_time_state: Sender<KeepTimeState>,
    pub refresh_next_due: Arc<Notify>,
}

// Used to pause the time keeping task during suspend
pub enum KeepTimeState {
    Awake,
    Sleeping,
}

async fn dbus_suspend_events() -> zbus::Result<PrepareForSleepStream> {
    use zbus::Connection;
    let connection = Connection::system().await?;
    let manager = ManagerProxy::new(&connection).await?;

    manager.receive_prepare_for_sleep().await
}

impl DaemonCtx {
    pub fn get_timerinfo_for_client(&self, now: Instant) -> Vec<TimerInfoForClient> {
        self.timers.get_timerinfo_for_client(now)
    }

    // Whenever the system goes to sleep we
    // - pause the time keeping task
    // - track how long we've been asleep
    // - on wake, elapse and notify those that should have elapsed while asleep
    // - resume the time keeping task
    pub async fn monitor_dbus_suspend_events(&self) -> zbus::Result<()> {
        let mut stream = dbus_suspend_events().await?;

        loop {
            //
            // Handle Suspend
            //
            let Some(signal) = stream.next().await else {
                break;
            };
            let start = signal.args()?.start;
            log::trace!("Received PrepareForSleep(start={start})");
            // Expect to suspend
            if !start {
                log::warn!("Received wake signal without preceding sleep. Ignoring.");
                continue;
            }
            let slept_at = SystemTime::now();
            if let Err(err) = self.tx_keep_time_state.send(KeepTimeState::Sleeping).await {
                log::error!("Failed to send sleep state: {}", err);
            }

            log::info!("System is preparing to sleep.");

            //
            // Then Handle awake
            //
            let Some(signal) = stream.next().await else {
                break;
            };
            let start = signal.args()?.start;
            // Expect to wake
            if start {
                log::warn!("Received sleep signal again before waking. Ignoring");
                continue;
            }
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
            if let Err(err) = self.tx_keep_time_state.send(KeepTimeState::Awake).await {
                log::error!("Failed to send keep_time_state message: {}", err);
            };
            for timer_id in elapsed_while_sleeping {
                self.tx_elapsed_events
                    .send(ElapsedEvent(timer_id))
                    .await
                    .expect("elapsed event receiver was closed");
            }
        }

        log::warn!(concat!(
            "D-Bus connection was lost.\n",
            "Sand will be unable to correctly handle system sleep."
        ));

        Ok(())
    }

    // TODO we can eliminate the keep_time_state channel and the monitor
    // dbus_supend_events thread by just awaiting receive_prepare_for_sleep()
    // in our select!
    pub async fn keep_time(&self, mut rx_keep_time_state: Receiver<KeepTimeState>) -> ! {
        let mut state = KeepTimeState::Awake;

        loop {
            match state {
                KeepTimeState::Sleeping => {
                    state = rx_keep_time_state
                        .recv()
                        .await
                        .expect("Bug: KeepTimeState channel closed");
                }
                KeepTimeState::Awake => {
                    if let Some((timer_id, next_due)) = self.timers.next_due_running() {
                        tokio::select! {
                            Some(new_state) = rx_keep_time_state.recv() => {
                                state = new_state;
                            }
                            _ = self.refresh_next_due.notified() => {},
                            _ = tokio::time::sleep(next_due) => {
                                // todo: just merge the ElapsedEvent handler task with this one
                                self.tx_elapsed_events
                                    .send(ElapsedEvent(timer_id))
                                    .await
                                    .expect("elapsed event receiver was closed");
                                log::info!("Timer {timer_id} completed");
                                self.timers.remove(&timer_id);
                            }
                        }
                    } else {
                        tokio::select! {
                            Some(new_state) = rx_keep_time_state.recv() => {
                                state = new_state;
                            }
                            _ = self.refresh_next_due.notified() => {},

                        }
                    }
                }
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
