use std::convert::AsRef;
use std::fmt::Debug;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rodio::{self, OutputStream, OutputStreamHandle, StreamError};

// thanks sinesc
// https://github.com/RustAudio/rodio/issues/141#issuecomment-383371609
#[derive(Debug, Clone)]
pub struct Sound(Arc<[u8]>);

impl AsRef<[u8]> for Sound {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Sound {
    pub fn load<P>(path: P) -> io::Result<Sound>
    where
        P: AsRef<Path> + Debug,
    {
        use std::fs::File;
        let mut buf = Vec::with_capacity(1000000);
        let mut file = File::open(path)?;
        file.read_to_end(&mut buf)?;
        Ok(Sound(Arc::from(buf)))
    }

    fn cursor(self: &Self) -> io::Cursor<Sound> {
        io::Cursor::new(self.clone())
    }

    pub fn decoder(self: &Self) -> rodio::Decoder<io::Cursor<Sound>> {
        rodio::Decoder::new(self.cursor()).unwrap()
    }

    pub fn play(&self) {
        let decoder = self.decoder();
        tokio::spawn(async move {
            let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
            let sink = rodio::Sink::try_new(&handle).unwrap();
            sink.append(decoder);
            sink.sleep_until_end();
        });
        eprintln!("notification sound");
    }
}

struct AudioPlayer {
    stream: OutputStream,
    handle: OutputStreamHandle,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, StreamError> {
        let (stream, handle) = rodio::OutputStream::try_default()?;
        Ok(Self { stream, handle })
    }

    pub fn play(&self, sound: &Sound) {
        let decoder = sound.decoder();
        let sink = rodio::Sink::try_new(&self.handle).unwrap();
        sink.append(decoder);
        sink.detach();
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

pub struct ElapsedSoundPlayer {
    sound: Sound,
    player: AudioPlayer,
}

fn load_elapsed_sound() -> io::Result<Sound> {
    if let Some(ref xdg_path) = xdg_sand_data_dir() {
        let sound = Sound::load(xdg_path);
        if sound.is_ok() {
            return sound;
        }
    }
    Sound::load(usrshare_sound_path())
}

#[derive(Debug)]
enum ElapsedSoundPlayerError {
    IO(io::Error),
    Stream(StreamError),
}

impl ElapsedSoundPlayer {
    pub fn load() -> Result<ElapsedSoundPlayer, ElapsedSoundPlayerError> {
        use ElapsedSoundPlayerError as E;
        let sound = load_elapsed_sound().map_err(|e| E::IO(e))?;
        Self::new(sound).map_err(|e| E::Stream(e))
    }

    pub fn new(sound: Sound) -> Result<Self, StreamError> {
        Ok(Self {
            sound,
            player: AudioPlayer::new()?
        })
    }

    pub fn play(&self) {
        self.player.play(&self.sound);
    }
}