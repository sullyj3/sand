mod audio;
mod ctx;
mod handle_client;

use indoc::indoc;
use notify_rust::Notification;
use std::env::VarError;
use std::fmt::Display;
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

// TODO create module for getting socket: listen.rs or something

#[derive(Debug)]
enum GetSocketError {
    VarError(VarError),
    /// Returned when the environment variables `LISTEN_FDS` and `LISTEN_PID`
    /// are not set.
    NoListenPID,
    NoListenFDs,
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

impl Display for GetSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GetSocketError::VarError(err) => {
                write!(f, "Failed to get environment variable: {}", err)
            }
            GetSocketError::ParseIntError(err) => write!(f, "Failed to parse integer: {}", err),
            GetSocketError::NoListenPID => write!(
                f,
                "The LISTEN_PID environment variable is not set, so we're not running in systemd socket activation mode."
            ),
            GetSocketError::NoListenFDs => write!(
                f,
                "The LISTEN_FDS environment variable is not set, so we're not running in systemd socket activation mode."
            ),
            GetSocketError::PIDMismatch => write!(
                f,
                "The LISTEN_PID environment variable does not match our PID"
            ),
        }
    }
}

// TODO I don't think we actually need this env variable. mutually redundant with SAND_SOCK_PATH
fn env_fd() -> Result<RawFd, GetSocketError> {
    let str_fd = std::env::var("SAND_SOCKFD")?;
    let fd = str_fd.parse::<RawFd>().inspect_err(|_err| {
        log::error!("Error: Found SAND_SOCKFD but couldn't parse it as an int")
    })?;
    Ok(fd)
}

fn systemd_socket_activation_fd() -> Result<RawFd, GetSocketError> {
    let listen_pid = std::env::var("LISTEN_PID")
        .map_err(|err| match err {
            VarError::NotPresent => GetSocketError::NoListenPID,
            _ => GetSocketError::VarError(err),
        })?
        .parse::<u32>()
        .expect("Couldn't parse LISTEN_PID as u32");
    let our_pid = std::process::id();

    if listen_pid != our_pid {
        log::trace!("LISTEN_PID does not match our PID");
        return Err(GetSocketError::PIDMismatch);
    }
    log::trace!("LISTEN_PID matches our PID");

    let listen_fds = std::env::var("LISTEN_FDS")
        .map_err(|err| match err {
            VarError::NotPresent => GetSocketError::NoListenFDs,
            _ => GetSocketError::VarError(err),
        })?
        .parse::<u32>()
        .expect("Couldn't parse LISTEN_FDS as u32");

    if listen_fds != 1 {
        log::warn!(
            "Expected LISTEN_FDS to be 1, but found {}. Continuing anyway",
            listen_fds
        );
    }
    Ok(SYSTEMD_SOCKFD)
}

fn get_fd() -> Option<RawFd> {
    env_fd().ok()
        .inspect(|_| log::debug!("Found SAND_SOCKFD"))
        .or_else(|| {
            log::debug!(
                "SAND_SOCKFD not found, falling back the default systemd socket file descriptor (3)."
            );
            systemd_socket_activation_fd().inspect_err(|err|
                log::error!("Failed to get systemd socket file descriptor: {}", err)
            ).ok()
         })
}

fn maybe_delete_stale_socket(path: &PathBuf) {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) => {
            match e.kind() {
                io::ErrorKind::NotFound => {}
                _ => log::error!(
                    indoc! {"
                        While trying to delete potential stale sockets:
                        Failed to get metadata at socket path {:?}: {}"},
                    path,
                    e
                ),
            }
            return;
        }
    };

    if !meta.file_type().is_socket() {
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

    // safe to remove stale socket
    if let Err(e) = std::fs::remove_file(path) {
        log::error!("Failed to remove existing socket {:?}: {}", path, e);
    } else {
        log::debug!("Removed stale socket at {:?}", path);
    }
}

/// Get a UnixListener for accepting client connections.
///
/// Since this calls UnixListener::bind, it must be called from within a tokio
/// runtime.
fn get_socket() -> io::Result<tokio::net::UnixListener> {
    env_sock_path()
        .inspect(|path| {
            log::trace!("found path in SAND_SOCK_PATH: {:?}", path);
            maybe_delete_stale_socket(path);
        })
        .map(tokio::net::UnixListener::bind)
        .unwrap_or_else(|| {
            let Some(fd) = get_fd() else {
                log::error!(indoc! {"
                    Since we didn't get SAND_SOCKFD, SAND_SOCK_PATH, or LISTEN_PID and LISTEN_FDS,
                    I don't know what socket to listen on! Exiting..."});
                std::process::exit(1);
            };
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
