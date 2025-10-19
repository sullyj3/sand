mod handle_client;
mod ctx;

use std::io;
use std::mem;
use std::os::fd::FromRawFd;
use std::os::fd::RawFd;
use std::os::unix;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use notify_rust::Notification;
use tokio;
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;

use crate::cli;
use crate::sand::audio::ElapsedSoundPlayer;
use crate::sand::socket::env_sock_path;
use crate::sand::timer::TimerId;
use handle_client::handle_client;
use ctx::DaemonCtx;

const SYSTEMD_SOCKFD: RawFd = 3;

fn env_fd() -> Option<u32> {
    let str_fd = std::env::var("SAND_SOCKFD").ok()?;
    let fd = str_fd
        .parse::<u32>()
        .expect("Error: Found SAND_SOCKFD but couldn't parse it as a string")
        .into();
    Some(fd)
}

fn get_fd() -> RawFd {
    match env_fd() {
        None => {
            log::debug!("SAND_SOCKFD not found, falling back the default systemd socket file descriptor (3).");
            SYSTEMD_SOCKFD
        }
        Some(fd) => {
            log::debug!("Found SAND_SOCKFD.");
            fd.try_into()
                .expect("Error: SAND_SOCKFD is too large to be a file descriptor.")
        }
    }
}

async fn accept_loop(
    listener: UnixListener,
    ctx: DaemonCtx,
) {
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

fn get_socket() -> io::Result<UnixListener> {
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
                        "SAND_SOCK_PATH {:?} exists but is not a socket.\n\
                         (type: {:?})\n\
                         Refusing to overwrite — please remove or change SAND_SOCK_PATH."
                        , path, meta.file_type());
                        
                    std::process::exit(1);
                }
            }

        })
        .map(UnixListener::bind)
        .unwrap_or_else(|| {
            let fd = get_fd();
            let std_listener: unix::net::UnixListener = unsafe {
                unix::net::UnixListener::from_raw_fd(fd) 
            };
            std_listener.set_nonblocking(true)?;
            UnixListener::from_std(std_listener)
    })
}

async fn daemon() -> io::Result<()> {
    let mut log_builder = colog::default_builder();
    if std::env::var("RUST_LOG").is_err() {
        if cfg!(debug_assertions) {
            log_builder.filter_level(log::LevelFilter::Debug);
        };
    }
    log_builder.init();
    log::info!("Starting sand daemon v{}", env!("CARGO_PKG_VERSION"));

    let (tx_elapsed_events, rx_elapsed_events) = mpsc::channel(20);
    tokio::spawn(notifier_thread(rx_elapsed_events));
    let listener: UnixListener = get_socket()?;
    log::info!("Daemon started.");
    accept_loop(listener, DaemonCtx {
        timers: Default::default(),
        tx_elapsed_events,
    }).await;

    Ok(())
}

struct ElapsedEvent(TimerId);

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
        if let Err(e) = player.play() {
            log::error!("Error playing timer elapsed sound: {e}");
        }
    } else {
        log::debug!("player is None - not playing sound");
    }
}

async fn notifier_thread(mut elapsed_events: Receiver<ElapsedEvent>) -> ! {
    let stream_handle = match rodio::OutputStream::try_default() {
        Ok((stream, handle)) => {
            mem::forget(stream);
            Some(handle)
        }
        Err(e) => {
            log::debug!("Failed to initialise OutputStream:\n{:?}", e);
            None
        }
    };

    log::trace!("stream_handle is {}", 
        if stream_handle.is_some() {"some"} else {"none"});

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

    while let Some(ElapsedEvent(timer_id)) = elapsed_events.recv().await {
        let player = player.clone();
        tokio::spawn(async move {
            do_notification(player, timer_id);
        });
    }
    unreachable!("bug: elapsed_events channel was closed.")
}

pub fn main(_args: cli::DaemonArgs) -> io::Result<()> {
    tokio::runtime::Runtime::new()?.block_on(daemon())
}
