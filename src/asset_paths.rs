use std::{
    env,
    path::{Path, PathBuf},
};

const ASSETS_DIR: &str = "assets";

pub fn runtime_asset_path(relative_path: impl AsRef<Path>) -> PathBuf {
    runtime_assets_dir().join(relative_path)
}

fn runtime_assets_dir() -> PathBuf {
    if let Some(path) = env::var_os("BEVY_ASSET_ROOT")
        .map(PathBuf::from)
        .and_then(normalize_assets_dir)
    {
        return path;
    }

    if let Ok(cwd) = env::current_dir() {
        if let Some(path) = normalize_assets_dir(cwd) {
            return path;
        }
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            if let Some(path) = normalize_assets_dir(exe_dir.to_path_buf()) {
                return path;
            }
        }
    }

    PathBuf::from(ASSETS_DIR)
}

fn normalize_assets_dir(base: PathBuf) -> Option<PathBuf> {
    if base.join(ASSETS_DIR).is_dir() {
        return Some(base.join(ASSETS_DIR));
    }

    if base.is_dir() && base.file_name().is_some_and(|name| name == ASSETS_DIR) {
        return Some(base);
    }

    None
}
