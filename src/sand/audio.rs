use std::convert::AsRef;
use std::fmt::Debug;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rodio::OutputStreamHandle;
use rodio::Source;

use crate::sand::PKGNAME;

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct Sound {
    data: Arc<[u8]>,
}

impl AsRef<[u8]> for Sound {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl Sound {
    pub fn load<P>(path: P) -> io::Result<Self>
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

    pub fn play(&self, handle: &OutputStreamHandle) -> Result<(), rodio::PlayError> {
        let decoder = self.decoder();
        handle.play_raw(decoder.convert_samples())
    }
}

// TODO weren't we using opus? look into which is better and whether opus is supported
const SOUND_FILENAME: &str = "timer_sound.flac";

fn xdg_sand_data_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(PKGNAME))
}

fn xdg_sound_path() -> Option<PathBuf> {
    Some(xdg_sand_data_dir()?.join(SOUND_FILENAME))
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
    path
}

fn load_elapsed_sound() -> io::Result<Sound> {
    if let Some(ref xdg_path) = xdg_sound_path() {
        let sound = Sound::load(xdg_path);
        if sound.is_ok() {
            return sound;
        }
    }
    Sound::load(default_sound_path())
}

#[derive(Clone)]
pub struct ElapsedSoundPlayer {
    sound: Sound,
    handle: OutputStreamHandle,
}

impl ElapsedSoundPlayer {
    pub fn new(handle: OutputStreamHandle) -> io::Result<Self> {
        load_elapsed_sound().inspect_err(|e| {
            log::warn!("Error loading the audio file: {}", e);
        }).map(|sound| {
            Self {sound, handle}
        })
    }

    pub fn play(&self) -> Result<(), rodio::PlayError> {
        self.sound.play(&self.handle)
    }
}