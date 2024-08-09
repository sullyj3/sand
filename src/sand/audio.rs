use std::convert::AsRef;
use std::fmt::Debug;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rodio::OutputStreamHandle;
use rodio::Source;

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
        // the intermediate vec and copy from vec to arc can probably be 
        // eliminated using unsafe.
        // It's not a big deal though
        let mut buf = Vec::with_capacity(1_000_000);
        let mut file = File::open(path)?;
        file.read_to_end(&mut buf)?;
        Ok(Self {
            data: Arc::from(buf),
        })
    }

    pub fn cursor(&self) -> io::Cursor<Self> {
        io::Cursor::new(self.clone())
    }

    pub fn decoder(&self) -> rodio::Decoder<io::Cursor<Self>> {
        rodio::Decoder::new(self.cursor()).unwrap()
    }

    pub fn play(&self, handle: &OutputStreamHandle) -> Result<(), rodio::PlayError> {
        let decoder = self.decoder();
        handle.play_raw(decoder.convert_samples())
    }
}

const SOUND_FILENAME: &str = "timer_sound.flac";

fn xdg_sand_data_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("sand"))
}

fn xdg_sound_path() -> Option<PathBuf> {
    Some(xdg_sand_data_dir()?.join(SOUND_FILENAME))
}

fn usrshare_sound_path() -> PathBuf {
    Path::new("/usr/share/sand").join(SOUND_FILENAME)
}

fn load_elapsed_sound() -> io::Result<Sound> {
    if let Some(ref xdg_path) = xdg_sound_path() {
        let sound = Sound::load(xdg_path);
        if sound.is_ok() {
            return sound;
        }
    }
    Sound::load(usrshare_sound_path())
}

#[derive(Clone)]
pub struct ElapsedSoundPlayer {
    sound: Sound,
    handle: OutputStreamHandle,
}

impl ElapsedSoundPlayer {
    pub fn new(handle: OutputStreamHandle) -> io::Result<Self> {
        let sound = load_elapsed_sound()?;
        Ok(Self {
            sound,
            handle,
        })
    }

    pub fn play(&self) -> Result<(), rodio::PlayError> {
        self.sound.play(&self.handle)
    }
}