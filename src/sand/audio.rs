use std::convert::AsRef;
use std::fmt::Debug;
use std::fmt::{self, Display, Formatter};
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rodio::OutputStream;

use crate::sand::PKGNAME;

#[derive(Debug)]
pub(crate) enum SoundLoadError {
    UnexpectedIO(io::Error),
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

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct SoundHandle {
    data: Arc<[u8]>,
}

impl AsRef<[u8]> for SoundHandle {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl SoundHandle {
    pub fn load<P>(path: P) -> SoundLoadResult<Self>
    where
        P: AsRef<Path>,
    {
        use std::fs::File;
        let mut buf = Vec::with_capacity(1_000_000);
        File::open(path)?.read_to_end(&mut buf)?;
        Ok(Self {
            data: Arc::from(buf),
        })
    }

    pub fn cursor(&self) -> io::Cursor<Self> {
        io::Cursor::new(self.clone())
    }

    pub fn decoder(&self) -> rodio::Decoder<io::Cursor<Self>> {
        rodio::Decoder::new(self.cursor()).expect("Failed to decode the sound")
    }
}

const SOUND_FILENAME: &str = "timer_sound";

fn sand_user_data_dir() -> SoundLoadResult<PathBuf> {
    dirs::data_dir()
        .ok_or(SoundLoadError::DataDirUnsupported)
        .map(|dir| dir.join(PKGNAME))
}

fn user_sound_path() -> SoundLoadResult<PathBuf> {
    let path = sand_user_data_dir()?
        .join(SOUND_FILENAME)
        .with_extension("flac");
    Ok(path)
}

fn load_user_sound() -> SoundLoadResult<SoundHandle> {
    let path = user_sound_path()?;
    log::debug!("Attempting to user load sound from {}", path.display());
    let sound = SoundHandle::load(&path);
    if sound.is_ok() {
        log::info!("Loaded user sound from {}", path.display());
    }
    sound
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

fn load_default_sound() -> SoundLoadResult<SoundHandle> {
    log::debug!("Attempting to load sound from default path");
    let path = default_sound_path();
    let sound = SoundHandle::load(&path);
    if sound.is_ok() {
        log::info!("Loaded default sound from {}", path.display());
    }
    sound
}

fn load_elapsed_sound() -> SoundLoadResult<SoundHandle> {
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
            SoundLoadError::DataDirUnsupported => {
                log::error!("{err}");
                load_default_sound()
            }
        },
    }
}

#[derive(Clone)]
pub struct ElapsedSoundPlayer {
    sound: SoundHandle,
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
        let decoder = self.sound.decoder();
        self.output_stream.mixer().add(decoder);
    }
}
