use std::error;
use std::fmt;
use std::sync::{PoisonError, RwLockReadGuard, RwLockWriteGuard};

use crate::domain::{RepositoryError, IOError};

#[derive(Debug)]
pub enum MemoryPersistenceError {
    ReadingError,
    WritingError,
}

impl error::Error for MemoryPersistenceError {}
impl IOError for MemoryPersistenceError {}

impl fmt::Display for MemoryPersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let error_type = match *self {
            MemoryPersistenceError::ReadingError => "reading",
            MemoryPersistenceError::WritingError => "writing",
        };

        write!(f, "Error occurred when trying to acquire lock for: {}", error_type)
    }
}

impl<T> From<PoisonError<RwLockReadGuard<'_, T>>> for RepositoryError {
    fn from(_: PoisonError<RwLockReadGuard<T>>) -> Self {
        RepositoryError::PersistenceError(
            Box::new(
                MemoryPersistenceError::ReadingError
            )
        )
    }
}

impl<T> From<PoisonError<RwLockWriteGuard<'_, T>>> for RepositoryError {
    fn from(_: PoisonError<RwLockWriteGuard<T>>) -> Self {
        RepositoryError::PersistenceError(
            Box::new(
                MemoryPersistenceError::WritingError
            )
        )
    }
}