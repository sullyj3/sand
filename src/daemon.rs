mod handle_client;
mod ctx;

use std::io;
use std::mem;
use std::os::fd::FromRawFd;
use std::os::fd::RawFd;
use std::os::unix;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use async_scoped;
use async_scoped::TokioScope;
use rodio::OutputStream;
use tokio;
use tokio::net::UnixListener;
use tokio::runtime::Runtime;

use crate::cli;
use crate::sand::socket::env_sock_path;
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

async fn accept_loop(listener: UnixListener, state: &DaemonCtx) {
    log::info!("Starting accept loop");
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                log::trace!("Got client");

                // Todo can we get rid of this clone? maybe if we use scoped threads?
                let _jh = tokio::spawn(handle_client(stream, state.clone()));
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

    let o_handle = match OutputStream::try_default() {
        Ok((stream, handle)) => {
            mem::forget(stream);
            Some(handle)
        }
        Err(e) => {
            log::debug!("Failed to initialise OutputStream:\n{:?}", e);
            None
        }
    };

    let state = DaemonCtx::new(o_handle);
    let listener: UnixListener = get_socket()?;

    log::info!("Daemon started.");
    TokioScope::scope_and_block(|scope| {
        scope.spawn(accept_loop(listener, &state));
    });

    Ok(())
}

pub fn main(_args: cli::DaemonArgs) -> io::Result<()> {
    Runtime::new()?.block_on(daemon())
}
