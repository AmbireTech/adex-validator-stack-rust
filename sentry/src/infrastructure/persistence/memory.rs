use std::error;
use std::fmt;
use std::sync::{PoisonError, RwLockReadGuard, RwLockWriteGuard};

use domain::{IOError, RepositoryError};

#[derive(Debug)]
pub enum MemoryPersistenceError {
    Reading,
    Writing,
}

impl error::Error for MemoryPersistenceError {}
impl IOError for MemoryPersistenceError {}

impl fmt::Display for MemoryPersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let error_type = match *self {
            MemoryPersistenceError::Reading => "reading",
            MemoryPersistenceError::Writing => "writing",
        };

        write!(f, "Error occurred when trying to acquire lock for: {}", error_type)
    }
}

impl<T> From<PoisonError<RwLockReadGuard<'_, T>>> for MemoryPersistenceError {
    fn from(_: PoisonError<RwLockReadGuard<T>>) -> Self {
        MemoryPersistenceError::Reading
    }
}

impl<T> From<PoisonError<RwLockWriteGuard<'_, T>>> for MemoryPersistenceError {
    fn from(_: PoisonError<RwLockWriteGuard<T>>) -> Self {
        MemoryPersistenceError::Writing
    }
}

impl Into<RepositoryError> for MemoryPersistenceError {
    fn into(self) -> RepositoryError {
        RepositoryError::IO(
            Box::new(self)
        )
    }
}