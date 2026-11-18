use std::path::{Path, PathBuf};

/// Errors produced by the Blurlist engine.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not read {path}: {reason}")]
    UnsupportedImage { path: PathBuf, reason: String },

    #[error("image error: {0}")]
    Image(String),

    #[error("model error: {0}")]
    Model(String),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Invalid(String),

    #[error("the operation was cancelled")]
    Cancelled,
}

impl Error {
    pub fn io(path: &Path, source: std::io::Error) -> Self {
        Error::Io { path: path.to_path_buf(), source }
    }

    pub fn decode(path: &Path, err: image::ImageError) -> Self {
        Error::UnsupportedImage { path: path.to_path_buf(), reason: err.to_string() }
    }
}

impl From<ort::Error> for Error {
    fn from(e: ort::Error) -> Self {
        Error::Model(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Invalid(format!("invalid JSON: {e}"))
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
