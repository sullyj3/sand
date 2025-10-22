use std::path::PathBuf;

pub fn env_sock_path() -> Option<PathBuf> {
    std::env::var("SAND_SOCK_PATH").map(Into::into).ok()
}

pub fn default_sock_path() -> Option<PathBuf> {
    dirs::runtime_dir().map(|p| p.join("sand.sock"))
}

pub fn get_sock_path() -> Option<PathBuf> {
    env_sock_path().or_else(default_sock_path)
}
