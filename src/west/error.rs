use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WestError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("could not find a Zephyr workspace (.west/ directory) from {0}")]
    WorkspaceNotFound(PathBuf),

    #[error("failed to parse west manifest: {0}")]
    ManifestParse(#[from] serde_yaml::Error),

    #[error("git command failed: {0}")]
    Git(String),

    #[error("could not determine cache directory")]
    NoCacheDir,
}
