use domain::{IOError, RepositoryError};
use std::{error, fmt};

#[derive(Debug)]
pub enum ApiPersistenceError {
    Reading,
    Writing,
}

impl error::Error for ApiPersistenceError {}
impl IOError for ApiPersistenceError {}

impl fmt::Display for ApiPersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error_type = match *self {
            ApiPersistenceError::Reading => "reading",
            ApiPersistenceError::Writing => "writing",
        };

        write!(
            f,
            "Error occurred when trying to acquire lock for: {}",
            error_type
        )
    }
}

impl Into<RepositoryError> for ApiPersistenceError {
    fn into(self) -> RepositoryError {
        RepositoryError::IO(Box::new(self))
    }
}
