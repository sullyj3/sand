use std::path::PathBuf;

pub fn get_sock_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("SAND_SOCK_PATH") {
        Some(path.into())
    } else {
        Some(dirs::runtime_dir()?.join("sand.sock"))
    }
}
