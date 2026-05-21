use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] rusqlite_migration::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("not found")]
    NotFound,
    #[error("invalid: {0}")]
    Invalid(String),
}

impl Error {
    /// Map a `rusqlite` error from a single-row lookup: a missing row becomes
    /// [`Error::NotFound`]; any other error is forwarded unchanged.
    pub(crate) fn from_lookup(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound,
            other => other.into(),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
