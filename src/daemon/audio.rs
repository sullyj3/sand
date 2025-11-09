use std::ffi::OsStr;
use std::fmt::Debug;
use std::fmt::{self, Display, Formatter};
use std::io::{self, Cursor, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

use indoc::indoc;
use notify::{RecursiveMode, Watcher as _};
use rodio::decoder::LoopedDecoder;
use rodio::source::Buffered;
use rodio::{Decoder, OutputStream, Sink, Source};
use tokio::sync::{Mutex, MutexGuard, RwLock};
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::ReceiverStream;

use crate::sand::PKGNAME;

#[derive(Debug)]
pub(crate) enum SoundLoadError {
    UnexpectedIO(io::Error),
    DecoderError(String),
    NotFound,
    DataDirUnsupported,
}

impl Display for SoundLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            SoundLoadError::UnexpectedIO(error) => {
                write!(f, "Unexpected IO error: {}", error)
            }
            SoundLoadError::NotFound => {
                write!(f, "Sound file not found")
            }
            SoundLoadError::DataDirUnsupported => {
                write!(f, "System does not support a user data directory")
            }
            SoundLoadError::DecoderError(err) => {
                write!(f, "Decoder error: {}", err)
            }
        }
    }
}

impl From<io::Error> for SoundLoadError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            ErrorKind::NotFound => SoundLoadError::NotFound,
            _ => SoundLoadError::UnexpectedIO(error),
        }
    }
}

type SoundLoadResult<T> = Result<T, SoundLoadError>;

// type Sound = Buffered<Decoder<BufReader<File>>>;
type LoopedSound = Buffered<LoopedDecoder<Cursor<Vec<u8>>>>;

fn load_sound(path: &Path) -> SoundLoadResult<LoopedSound> {
    let buf = {
        use std::fs::File;
        let mut file = File::open(path)?;
        log::debug!(
            "Found sound file at {}, attempting to load",
            path.to_string_lossy()
        );
        let mut buf = Vec::with_capacity(1_000_000);
        file.read_to_end(&mut buf)?;
        buf
    };
    let cursor = Cursor::new(buf);
    let decoder =
        Decoder::new_looped(cursor).map_err(|err| SoundLoadError::DecoderError(err.to_string()))?;
    // let decoder =
    //     Decoder::try_from(file).map_err(|err| SoundLoadError::DecoderError(err.to_string()))?;
    let buffered = decoder.buffered();
    Ok(buffered)
}

const SOUND_FILENAME: &str = "timer_sound";

fn sand_user_data_dir() -> SoundLoadResult<PathBuf> {
    dirs::data_dir()
        .ok_or(SoundLoadError::DataDirUnsupported)
        .map(|dir| dir.join(PKGNAME))
}

fn user_sound_path() -> SoundLoadResult<PathBuf> {
    let path = sand_user_data_dir()?.join(SOUND_FILENAME);
    Ok(path)
}

const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "aac", "m4a", "ogg"];

fn load_user_sound() -> SoundLoadResult<LoopedSound> {
    let path_no_extension = user_sound_path()?;
    log::debug!(
        "Attempting to load user sound from {}.*",
        path_no_extension.display()
    );
    // TODO .ogg doesn't seem to be working
    SUPPORTED_EXTENSIONS
        .iter()
        .find_map(|extension| {
            log::trace!("Trying extension: {}", extension);
            let path = path_no_extension.with_extension(extension);
            match load_sound(&path) {
                Ok(sound) => {
                    log::info!("Loaded user sound from {}", path.display());
                    Some(Ok(sound))
                }
                Err(err) => match err {
                    SoundLoadError::NotFound => None,
                    _ => Some(Err(err)),
                },
            }
        })
        .unwrap_or(Err(SoundLoadError::NotFound))
}

// TODO fix this mess
fn load_default_sound() -> SoundLoadResult<LoopedSound> {
    log::debug!("Attempting to load sound from default path");

    if cfg!(debug_assertions) {
        log::info!("target is debug, loading sound relative to current working directory");
        let mut path = PathBuf::from("./resources").join(SOUND_FILENAME);
        path.add_extension("flac");
        let sound = load_sound(&path);
        match &sound {
            Ok(_) => log::info!("Loaded default sound from {}", path.display()),
            Err(err) => log::error!(
                "Failed to load default sound from {}: {}",
                path.display(),
                err
            ),
        }
        sound
    } else {
        // TODO compile PREFIX into the binary instead of checking both at runtime
        {
            log::trace!("target is release, attempting to load sound from /usr/share");
            let mut path = Path::new("/usr/share").join(PKGNAME);
            path.push(SOUND_FILENAME);
            path.add_extension("flac");
            match load_sound(&path) {
                Ok(sound) => {
                    log::info!("Loaded default sound from {}", path.display());
                    return Ok(sound);
                }
                Err(err) => {
                    log::debug!("Failed to load default sound from /usr/share: {}", err)
                }
            }
        }

        {
            log::trace!("Attempting to load sound from /usr/local/share");
            let mut path = Path::new("/usr/local/share").join(PKGNAME);
            path.push(SOUND_FILENAME);
            path.add_extension("flac");
            let sound = load_sound(&path);
            match sound {
                Ok(sound) => {
                    log::info!("Loaded default sound from {}", path.display());
                    return Ok(sound);
                }
                Err(ref err) => {
                    log::debug!(
                        "Failed to load default sound from /usr/local/share: {}",
                        err
                    )
                }
            }
            sound
        }
    }
}

fn load_elapsed_sound() -> SoundLoadResult<LoopedSound> {
    load_user_sound().or_else(|err| {
        match &err {
            SoundLoadError::NotFound => {
                log::debug!("User sound not found");
            }
            _ => {
                log::error!("Error loading user sound: {err}");
            }
        }
        load_default_sound()
    })
}

pub enum ElapsedSoundPlayerError {
    SoundLoadError(SoundLoadError),
    StreamError(rodio::StreamError),
}

impl From<SoundLoadError> for ElapsedSoundPlayerError {
    fn from(err: SoundLoadError) -> Self {
        Self::SoundLoadError(err)
    }
}

impl From<rodio::StreamError> for ElapsedSoundPlayerError {
    fn from(err: rodio::StreamError) -> Self {
        Self::StreamError(err)
    }
}

impl Display for ElapsedSoundPlayerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ElapsedSoundPlayerError::SoundLoadError(err) => {
                write!(f, "Error loading the audio file: {}", err)
            }
            ElapsedSoundPlayerError::StreamError(err) => {
                write!(f, "Failed to initialize OutputStream: {}", err)
            }
        }
    }
}

/// While any task holds one of these handles, the sound will continue to loop.
/// Once all handles are dropped, the sound will stop playing.
/// Only one instance of the sound will play at a time.
#[must_use]
pub struct LoopedSoundPlayback(
    // This field is not supposed to be accessed, we use its Drop for side
    // effects
    #[allow(dead_code)] Arc<Sink>,
);

pub struct ElapsedSoundPlayer {
    sound: Arc<RwLock<LoopedSound>>,
    output_stream: OutputStream,
    sink: Mutex<Weak<Sink>>,
}

impl ElapsedSoundPlayer {
    pub fn new() -> Result<Self, ElapsedSoundPlayerError> {
        let stream = rodio::OutputStreamBuilder::open_default_stream()
            .inspect_err(|e| log::debug!("{e}"))?;
        let sound = load_elapsed_sound().inspect_err(|e| log::warn!("{e}"))?;
        let sound = Arc::new(RwLock::new(sound));
        tokio::spawn(refresh_sound_when_changed(sound.clone()));

        let player = Self {
            sound: sound,
            output_stream: stream,
            sink: Mutex::new(Weak::new()),
        };
        Ok(player)
    }

    pub async fn play(&self) {
        let s = self.sound.read().await.clone();
        self.output_stream.mixer().add(s);
    }

    pub async fn play_looped(&self) -> LoopedSoundPlayback {
        let sink_lock = self.sink.lock().await;
        match sink_lock.upgrade() {
            Some(sink) => LoopedSoundPlayback(sink),
            None => {
                let sink = self.new_elapsed_sound_sink(sink_lock).await;
                LoopedSoundPlayback(sink)
            }
        }
    }

    async fn new_elapsed_sound_sink(&self, mut lock: MutexGuard<'_, Weak<Sink>>) -> Arc<Sink> {
        let mixer = self.output_stream.mixer();
        let sink = Sink::connect_new(mixer);
        let sound = self.sound.read().await.clone();
        sink.append(sound);
        let arc = Arc::new(sink);
        *lock = Arc::downgrade(&arc);
        arc
    }
}

async fn refresh_sound(sound: &RwLock<LoopedSound>) -> Result<(), ElapsedSoundPlayerError> {
    log::info!("Refreshing sound.");
    let new_sound = load_elapsed_sound()?;
    *sound.write().await = new_sound;
    Ok(())
}

async fn refresh_sound_when_changed(sound: Arc<RwLock<LoopedSound>>) {
    let data_dir: PathBuf = match sand_user_data_dir() {
        Ok(p) => p,
        Err(err) => {
            log::error!(
                indoc! {"
                Error obtaining path to user data directory: {}.
                Unable to start timer sound file watcher"},
                err
            );
            return;
        }
    };

    let (tx_file_events, rx_file_events) = tokio::sync::mpsc::channel(10);

    let handle_fs_event = move |ev| match ev {
        Ok(ev) => {
            log::trace!("File change event: {ev:?}");
            tx_file_events
                .blocking_send(ev)
                .expect("failed to send file event {ev:?}");
        }
        Err(err) => {
            log::warn!("Error from sound file watcher: {err}");
        }
    };
    let mut watcher = match notify::recommended_watcher(handle_fs_event) {
        Ok(w) => w,
        Err(err) => {
            log::error!("Error creating audio file watcher: {err}");
            return;
        }
    };
    if let Err(err) = watcher.watch(&data_dir, RecursiveMode::Recursive) {
        log::error!("Error starting audio file watcher: {err}");
        return;
    }

    let mut stream = ReceiverStream::new(rx_file_events).filter(|event| {
        (event.kind.is_create() || event.kind.is_modify())
            && event.paths.iter().any(|p| {
                let name_match = p.file_stem() == Some(OsStr::new("timer_sound"));
                let extension_match = p
                    .extension()
                    .is_some_and(|ext| SUPPORTED_EXTENSIONS.iter().any(|&sup_ext| sup_ext == ext));
                name_match && extension_match
            })
    });

    log::debug!("User sound file watcher started.");
    while let Some(_event) = stream.next().await {
        if let Err(e) = refresh_sound(&sound).await {
            log::warn!("{e}");
        }
    }
    log::error!("Bug: sound file events channel closed");
}
