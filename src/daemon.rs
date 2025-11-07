mod audio;
mod ctx;
mod handle_client;

use indoc::indoc;
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
use tokio::sync::RwLock;

use crate::cli;
use crate::daemon::audio::ElapsedSoundPlayer;
use crate::sand::socket::env_sock_path;
use ctx::DaemonCtx;
use handle_client::handle_client;

const SYSTEMD_SOCKFD: RawFd = 3;

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
    systemd_socket_activation_fd()
        .inspect_err(|err| {
            log::debug!("Failed to get systemd socket file descriptor:\n    {}", err)
        })
        .ok()
}

fn maybe_delete_stale_socket(path: &PathBuf) {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return,
        Err(e) => {
            log::error!(
                indoc! {"
                    While trying to delete potential stale sockets:
                    Failed to get metadata at socket path {:?}: {}"},
                path,
                e
            );
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
    if let Some(path) = env_sock_path() {
        log::trace!("found path in SAND_SOCK_PATH: {:?}", path);
        maybe_delete_stale_socket(&path);
        let listener = tokio::net::UnixListener::bind(path)?;
        return Ok(listener);
    }

    if let Some(fd) = get_fd() {
        let std_listener: unix::net::UnixListener =
            unsafe { unix::net::UnixListener::from_raw_fd(fd) };
        std_listener.set_nonblocking(true)?;
        let listener = tokio::net::UnixListener::from_std(std_listener)?;
        return Ok(listener);
    }

    log::error!(indoc! {"
        I don't know what socket to listen on!
        - We didn't get SAND_SOCK_PATH
        - Since we didn't get LISTEN_PID or LISTEN_FDS, we're not running in
          systemd socket activation mode.

        Please specify a socket with SAND_SOCK_PATH, or run using the provided
        systemd socket unit.

        Exiting."});
    std::process::exit(1);
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

    tokio::runtime::Runtime::new()?.block_on(daemon())
}

async fn daemon() -> io::Result<()> {
    let elapsed_sound_player = ElapsedSoundPlayer::new()
        .inspect(|_| log::debug!("ElapsedSoundPlayer successfully initialized."))
        .inspect_err(|_| {
            log::warn!(indoc! {"
                Failed to initialize elapsed sound player.
                There will be no timer sounds."})
        })
        .ok();

    let ctx = DaemonCtx {
        timers: Default::default(),
        refresh_next_due: Arc::new(Notify::new()),
        last_started: Arc::new(RwLock::new(None)),
        elapsed_sound_player,
    };

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
