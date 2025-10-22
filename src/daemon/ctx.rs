use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use dashmap::Entry;
use logind_zbus::manager::ManagerProxy;
use tokio::sync::mpsc::Sender;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;

use crate::daemon::ElapsedEvent;
use crate::sand::message;
use crate::sand::timer::Timer;
use crate::sand::timer::TimerId;
use crate::sand::timer::TimerInfoForClient;
use crate::sand::timers::Timers;

/// Should be cheap to clone.
#[derive(Clone)]
pub struct DaemonCtx {
    pub timers: Arc<Timers>,
    pub tx_elapsed_events: Sender<ElapsedEvent>,
}

impl DaemonCtx {
    pub fn get_timerinfo_for_client(&self, now: Instant) -> Vec<TimerInfoForClient> {
        self.timers.get_timerinfo_for_client(now)
    }

    async fn countdown(self, id: TimerId, duration: Duration, rx_added: Arc<Notify>) {
        tokio::time::sleep(duration).await;
        log::info!("Timer {id} completed");

        self.tx_elapsed_events
            .send(ElapsedEvent(id))
            .await
            .expect("elapsed event receiver was closed");
        // Since the countdown is started concurrently with adding the timer to
        // the map, we need to ensure that it has been added before we remove
        // it, in case the duration of the countdown is short or 0.
        rx_added.notified().await;
        self.timers.elapse(id)
    }

    // Whenever the system goes to sleep we
    // - cancel all async countdown tasks
    // - record running timers' remaining durations
    // - on wake, elapse and notify those that should have elapsed while asleep
    // - spawn new countdowns for the remaining duration before sleep less the sleep duration
    pub async fn monitor_dbus_suspend_events(&self) -> zbus::Result<()> {
        use zbus::Connection;
        let connection = Connection::system().await?;
        let manager = ManagerProxy::new(&connection).await?;

        let mut stream = manager.receive_prepare_for_sleep().await?;

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
            log::info!("System is preparing to sleep.");
            let running_timers: Vec<(TimerId, Duration)> = self.timers.cancel_running_countdowns();

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
            let woke_at_sys = SystemTime::now();
            let sleep_duration = match woke_at_sys.duration_since(slept_at) {
                Ok(dur) => dur,
                Err(err) => {
                    log::error!("When waking, system clock reported having gone backwards in time since sleeping by {:?}. Assuming no time passed", err.duration());
                    Duration::ZERO
                }
            };
            log::info!("System just woke up. Slept for {:?}", sleep_duration);
            let woke_at = Instant::now();

            // resume or elapse all running timers
            for (timer_id, remaining) in running_timers {
                if remaining < sleep_duration {
                    // Handle timers that elapsed while the system was asleep
                    self.timers.0.remove(&timer_id);
                    self.tx_elapsed_events
                        .send(ElapsedEvent(timer_id))
                        .await
                        .expect("elapsed event receiver was closed");
                } else {
                    // Handle timers that still have remaining time
                    let new_duration = remaining - sleep_duration;
                    let new_due = woke_at + new_duration;

                    let Entry::Occupied(mut entry) = self.timers.entry(timer_id) else {
                        log::error!("Bug while resuming from sleep: tried to replace countdown of nonexistent timer");
                        continue;
                    };

                    let Timer::Running { due, countdown } = entry.get_mut() else {
                        log::error!("Bug while resuming from sleep: tried to replace countdown of timer that wasn't running");
                        continue;
                    };

                    let (new_countdown, notify_added) =
                        self.spawn_countdown(timer_id, new_duration);
                    *due = new_due;
                    *countdown = new_countdown;
                    notify_added.notify_one();
                }
            }
        }

        log::warn!(concat!(
            "D-Bus connection was lost.\n",
            "Sand will be unable to correctly handle system sleep."
        ));

        Ok(())
    }

    fn spawn_countdown(&self, id: TimerId, duration: Duration) -> (JoinHandle<()>, Arc<Notify>) {
        // Once the countdown has elapsed, it removes its associated `Timer`
        // from the Timers map by calling `timers.elapse(id)`. For short
        // durations (eg 0), We need to synchronize to ensure it doesn't do
        // this til after it's been added. This is what `notify_added` is for.

        // I'm not thrilled with the `Notify` based solution, it feels a little
        // awkward. I'm not sure whether there's a better way.

        // this seems like exactly the sort of thing `waitmap` was designed for,
        // but it looks unmaintained

        // Possibly we could have the countdown notify some central thread that
        // it's done through a chan. Then the central thread could instead be
        // responsible for doing the notification, playing the sound and removing
        // the elapsed timer from the map. It's not obvious to me whether that
        // would be simpler. The central thread would still have to somehow
        // wait for the timer to be added before removing it.
        let notify_added = Arc::new(Notify::new());
        let rx_added = notify_added.clone();
        let join_handle = tokio::spawn(self.clone().countdown(id, duration, rx_added));
        (join_handle, notify_added)
    }

    pub fn add_timer(&self, now: Instant, duration: Duration) -> TimerId {
        let vacant = self.timers.first_vacant_entry();
        let id = *vacant.key();

        let (join_handle, notify_added) = self.spawn_countdown(id, duration);
        vacant.insert(Timer::Running {
            due: now + duration,
            countdown: join_handle,
        });
        notify_added.notify_one();
        id
    }

    pub fn pause_timer(&self, id: TimerId, now: Instant) -> message::PauseTimerResponse {
        use message::PauseTimerResponse as Resp;

        let dashmap::Entry::Occupied(mut entry) = self.timers.entry(id) else {
            return Resp::TimerNotFound;
        };
        let timer = entry.get_mut();

        use Timer as T;
        match timer {
            T::Running { due, countdown } => {
                countdown.abort();
                *timer = T::Paused {
                    remaining: *due - now,
                };
                Resp::Ok
            }
            T::Paused { remaining: _ } => Resp::AlreadyPaused,
        }
    }

    pub fn resume_timer(&self, id: TimerId, now: Instant) -> message::ResumeTimerResponse {
        use message::ResumeTimerResponse as Resp;

        let dashmap::Entry::Occupied(mut entry) = self.timers.entry(id) else {
            return Resp::TimerNotFound;
        };
        let timer = entry.get_mut();

        use Timer as T;
        match timer {
            T::Paused { remaining } => {
                let (join_handle, notify_added) = self.spawn_countdown(id, *remaining);
                *timer = T::Running {
                    due: now + *remaining,
                    countdown: join_handle,
                };
                notify_added.notify_one();
                Resp::Ok
            }
            T::Running {
                due: _,
                countdown: _,
            } => Resp::AlreadyRunning,
        }
    }

    pub fn cancel_timer(&self, id: TimerId) -> message::CancelTimerResponse {
        use message::CancelTimerResponse as Resp;

        let dashmap::Entry::Occupied(entry) = self.timers.entry(id) else {
            return Resp::TimerNotFound;
        };
        let timer = entry.get();
        match timer {
            Timer::Paused { remaining: _ } => {}
            Timer::Running { due: _, countdown } => countdown.abort(),
        }
        entry.remove();
        Resp::Ok
    }
}
