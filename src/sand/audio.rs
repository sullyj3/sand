// TODO this module should probably be in daemon
use std::fmt::Debug;
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io::{self, BufReader, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rodio::source::Buffered;
use rodio::{Decoder, OutputStream, Source};

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

type Sound = Buffered<Decoder<BufReader<File>>>;

fn load_sound(path: &Path) -> SoundLoadResult<Sound> {
    use std::fs::File;
    let file = File::open(path)?;
    log::debug!(
        "Found sound file at {}, attempting to load",
        path.to_string_lossy()
    );
    let decoder =
        Decoder::try_from(file).map_err(|err| SoundLoadError::DecoderError(err.to_string()))?;
    let buf = decoder.buffered();
    Ok(buf)
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

fn load_user_sound() -> SoundLoadResult<Sound> {
    let path_no_extension = user_sound_path()?;
    log::debug!(
        "Attempting to load user sound from {}.*",
        path_no_extension.display()
    );
    // TODO .ogg doesn't seem to be working
    const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "aac", "m4a", "ogg"];
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

fn default_sound_path() -> PathBuf {
    let mut path: PathBuf = if cfg!(debug_assertions) {
        log::info!("target is debug, loading sound relative to current working directory");
        PathBuf::from("./resources")
    } else {
        log::trace!("target is release, loading sound from /usr/share");
        Path::new("/usr/share").join(PKGNAME)
    };
    path.push(SOUND_FILENAME);
    path.add_extension("flac");
    path
}

fn load_default_sound() -> SoundLoadResult<Sound> {
    log::debug!("Attempting to load sound from default path");
    let path = default_sound_path();
    let sound = load_sound(&path);
    if sound.is_ok() {
        log::info!("Loaded default sound from {}", path.display());
    }
    sound
}

fn load_elapsed_sound() -> SoundLoadResult<Sound> {
    match load_user_sound() {
        Ok(sound) => Ok(sound),
        Err(err) => match &err {
            SoundLoadError::UnexpectedIO(unexpected_io_err) => {
                log::error!("While loading user sound: {}", unexpected_io_err);
                Err(err)
            }
            SoundLoadError::NotFound => {
                log::debug!("User sound not found");
                load_default_sound()
            }
            _ => {
                log::error!("{err}");
                load_default_sound()
            }
        },
    }
}

#[derive(Clone)]
pub struct ElapsedSoundPlayer {
    sound: Sound,
    output_stream: Arc<OutputStream>,
}

impl ElapsedSoundPlayer {
    pub fn new(handle: OutputStream) -> SoundLoadResult<Self> {
        load_elapsed_sound()
            .inspect_err(|e| {
                log::warn!("Error loading the audio file: {}", e);
            })
            .map(|sound| Self {
                sound,
                output_stream: Arc::new(handle),
            })
    }

    pub fn play(&self) {
        self.output_stream.mixer().add(self.sound.clone());
    }
}
