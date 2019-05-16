use std::error;
use std::fmt;

use bb8::RunError;

#[derive(Debug)]
pub enum PostgresPersistenceError {
    UserError(tokio_postgres::Error),
    TimedOut,
}

impl error::Error for PostgresPersistenceError {}

impl fmt::Display for PostgresPersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PostgresPersistenceError::UserError(ref error) => write!(f, "User error occurred: {}", error),
            PostgresPersistenceError::TimedOut => write!(f, "Timed out: attempted to get a connection"),
        }
    }
}

impl From<RunError<tokio_postgres::Error>> for PostgresPersistenceError {
    fn from(run_error: RunError<tokio_postgres::Error>) -> Self {
        match run_error {
            RunError::TimedOut => PostgresPersistenceError::TimedOut,
            RunError::User(error) => PostgresPersistenceError::UserError(error)
        }
    }
}