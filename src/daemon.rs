mod audio;
mod ctx;
mod handle_client;

use indoc::indoc;
use notify_rust::Notification;
use std::env::VarError;
use std::io;
use std::num::ParseIntError;
use std::os::fd::FromRawFd;
use std::os::fd::RawFd;
use std::os::unix;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio;
use tokio::sync::Notify;
use tokio::sync::mpsc;

use crate::cli;
use crate::sand::socket::env_sock_path;
use crate::sand::timer::TimerId;
use audio::ElapsedSoundPlayer;
use ctx::DaemonCtx;
use handle_client::handle_client;

const SYSTEMD_SOCKFD: RawFd = 3;

struct ElapsedEvent(TimerId);

/////////////////////////////////////////////////////////////////////////////////////////
// Setup
/////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
enum GetSocketError {
    VarError(VarError),
    ParseIntError(ParseIntError),
    PIDMismatch,
}

impl From<VarError> for GetSocketError {
    fn from(err: VarError) -> Self {
        GetSocketError::VarError(err)
    }
}

impl From<ParseIntError> for GetSocketError {
    fn from(err: ParseIntError) -> Self {
        GetSocketError::ParseIntError(err)
    }
}

fn env_fd() -> Result<RawFd, GetSocketError> {
    let str_fd = std::env::var("SAND_SOCKFD")?;
    let fd = str_fd.parse::<RawFd>().inspect_err(|_err| {
        log::error!("Error: Found SAND_SOCKFD but couldn't parse it as a string")
    })?;
    Ok(fd)
}

fn systemd_socket_activation_fd() -> Result<RawFd, GetSocketError> {
    Ok(SYSTEMD_SOCKFD)
}

fn get_fd() -> RawFd {
    env_fd().ok()
        .inspect(|_| log::debug!("Found SAND_SOCKFD"))
        .unwrap_or_else(|| {
            log::debug!(
                "SAND_SOCKFD not found, falling back the default systemd socket file descriptor (3)."
            );
            systemd_socket_activation_fd().unwrap()
         })
}

/// Get a UnixListener for accepting client connections.
///
/// Since this calls UnixListener::bind, it must be called from within a tokio
/// runtime.
fn get_socket() -> io::Result<tokio::net::UnixListener> {
    env_sock_path()
        .inspect(|path: &PathBuf| {
            log::trace!("found path in SAND_SOCK_PATH: {:?}", path);
            if let Ok(meta) = std::fs::symlink_metadata(path) {
                if meta.file_type().is_socket() {
                    // safe to remove stale socket
                    if let Err(e) = std::fs::remove_file(path) {
                        log::error!("Failed to remove existing socket {:?}: {}", path, e);
                    } else {
                        log::debug!("Removed stale socket at {:?}", path);
                    }
                } else {
                    log::error!(
                        indoc! {"
                        SAND_SOCK_PATH {:?} exists but is not a socket.
                            (type: {:?})
                        Refusing to overwrite — please remove or change SAND_SOCK_PATH."},
                        path,
                        meta.file_type()
                    );

                    std::process::exit(1);
                }
            }
        })
        .map(tokio::net::UnixListener::bind)
        .unwrap_or_else(|| {
            let fd = get_fd();
            let std_listener: unix::net::UnixListener =
                unsafe { unix::net::UnixListener::from_raw_fd(fd) };
            std_listener.set_nonblocking(true)?;
            tokio::net::UnixListener::from_std(std_listener)
        })
}

/////////////////////////////////////////////////////////////////////////////////////////
// Main
/////////////////////////////////////////////////////////////////////////////////////////

pub fn main(_args: cli::DaemonArgs) -> io::Result<()> {
    // Logging
    let mut log_builder = colog::default_builder();
    if std::env::var("RUST_LOG").is_err() {
        if cfg!(debug_assertions) {
            log_builder.filter_level(log::LevelFilter::Debug);
        };
    }
    log_builder.init();
    log::info!("Starting sand daemon v{}", env!("CARGO_PKG_VERSION"));

    // Channel for reporting elapsed timers
    let (tx_elapsed_events, rx_elapsed_events) = mpsc::channel(20);

    let ctx = DaemonCtx {
        timers: Default::default(),
        tx_elapsed_events,
        refresh_next_due: Arc::new(Notify::new()),
    };

    tokio::runtime::Runtime::new()?.block_on(daemon(ctx, rx_elapsed_events))
}

async fn daemon(ctx: DaemonCtx, rx_elapsed_events: mpsc::Receiver<ElapsedEvent>) -> io::Result<()> {
    // Generate notifications and sounds for elapsed timers
    tokio::spawn(notifier_thread(rx_elapsed_events));

    let c_ctx = ctx.clone();
    tokio::spawn(async move {
        c_ctx.keep_time().await;
    });

    let unix_listener: tokio::net::UnixListener = get_socket()?;
    client_accept_loop(unix_listener, ctx).await;
}

/////////////////////////////////////////////////////////////////////////////////////////
// Worker tasks
/////////////////////////////////////////////////////////////////////////////////////////

async fn notifier_thread(mut elapsed_events: mpsc::Receiver<ElapsedEvent>) -> ! {
    let player = ElapsedSoundPlayer::new()
        .inspect(|_| log::debug!("ElapsedSoundPlayer successfully initialized."))
        .inspect_err(|_| {
            log::warn!(indoc! {"
                Failed to initialize elapsed sound player.
                There will be no timer sounds."})
        })
        .ok();

    while let Some(ElapsedEvent(timer_id)) = elapsed_events.recv().await {
        let player = player.clone();
        tokio::spawn(async move {
            do_notification(player, timer_id);
        });
    }
    unreachable!("bug: elapsed_events channel was closed.")
}

async fn client_accept_loop(listener: tokio::net::UnixListener, ctx: DaemonCtx) -> ! {
    log::info!("Daemon started.");
    log::info!("Starting accept loop");
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                log::trace!("Got client");

                let _jh = tokio::spawn(handle_client(stream, ctx.clone()));
            }
            Err(e) => {
                log::error!("Failed to accept client: {}", e);
                continue;
            }
        };
    }
}

/////////////////////////////////////////////////////////////////////////////////////////
// Helpers
/////////////////////////////////////////////////////////////////////////////////////////

pub fn do_notification(player: Option<ElapsedSoundPlayer>, timer_id: TimerId) {
    let notification = Notification::new()
        .summary("Time's up!")
        .body(&format!("Timer {timer_id} has elapsed"))
        .icon("alarm")
        .urgency(notify_rust::Urgency::Critical)
        .show();
    if let Err(e) = notification {
        log::error!("Error showing desktop notification: {e}");
    }

    if let Some(ref player) = player {
        log::debug!("playing sound");
        player.play();
    } else {
        log::debug!("player is None - not playing sound");
    }
}
