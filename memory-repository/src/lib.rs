#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
use std::error;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use domain::{IOError, RepositoryError};

pub struct MemoryRepository<S: Clone> {
    records: Arc<RwLock<Vec<S>>>,
    cmp: Arc<dyn Fn(&S, &S) -> bool + Send + Sync>,
}

impl<S: Clone> MemoryRepository<S> {
    pub fn new(initial_records: &[S], cmp: Arc<dyn Fn(&S, &S) -> bool + Send + Sync>) -> Self {
        Self {
            records: Arc::new(RwLock::new(initial_records.to_vec())),
            cmp,
        }
    }

    pub fn list<F>(&self, limit: u32, page: u64, filter: F) -> Result<Vec<S>, MemoryRepositoryError>
    where
        F: Fn(&S) -> Option<S>,
    {
        // 1st page, start from 0
        let skip_results = ((page - 1) * limit as u64) as usize;
        // take `limit` results
        let take = limit as usize;

        self.records
            .read()
            .map(|reader| {
                reader
                    .iter()
                    .filter_map(|record| filter(record))
                    .skip(skip_results)
                    .take(take)
                    .collect()
            })
            .map_err(|error| MemoryRepositoryError::from(error))
    }

    pub fn list_all<F>(&self, filter: F) -> Result<Vec<S>, MemoryRepositoryError>
    where
        F: Fn(&S) -> Option<S>,
    {
        self.records
            .read()
            .map(|reader| reader.iter().filter_map(|record| filter(record)).collect())
            .map_err(|error| MemoryRepositoryError::from(error))
    }

    pub fn has(&self, record: &S) -> Result<bool, MemoryRepositoryError> {
        match self.records.read() {
            Ok(reader) => {
                let result = reader.iter().find(|current| (self.cmp)(current, record));
                Ok(result.is_some())
            }
            Err(error) => Err(MemoryRepositoryError::from(error)),
        }
    }

    pub fn add(&self, record: S) -> Result<(), MemoryRepositoryError> {
        if self.has(&record)? {
            Err(MemoryRepositoryError::AlreadyExists)
        } else {
            match self.records.write() {
                Ok(mut writer) => {
                    writer.push(record);

                    Ok(())
                }
                Err(error) => Err(MemoryRepositoryError::from(error)),
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum MemoryRepositoryError {
    Reading,
    Writing,
    AlreadyExists,
}

impl error::Error for MemoryRepositoryError {}

impl IOError for MemoryRepositoryError {}

impl fmt::Display for MemoryRepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error_type = match *self {
            MemoryRepositoryError::Reading => "reading",
            MemoryRepositoryError::Writing => "writing",
            MemoryRepositoryError::AlreadyExists => "already exist",
        };

        write!(
            f,
            "Error occurred when trying to acquire lock for: {}",
            error_type
        )
    }
}

impl<T> From<PoisonError<RwLockReadGuard<'_, T>>> for MemoryRepositoryError {
    fn from(_: PoisonError<RwLockReadGuard<'_, T>>) -> Self {
        MemoryRepositoryError::Reading
    }
}

impl<T> From<PoisonError<RwLockWriteGuard<'_, T>>> for MemoryRepositoryError {
    fn from(_: PoisonError<RwLockWriteGuard<'_, T>>) -> Self {
        MemoryRepositoryError::Writing
    }
}

impl Into<RepositoryError> for MemoryRepositoryError {
    fn into(self) -> RepositoryError {
        match &self {
            MemoryRepositoryError::Reading | MemoryRepositoryError::Writing => {
                RepositoryError::IO(Box::new(self))
            }
            // @TODO: Implement AlreadyExist Error
            MemoryRepositoryError::AlreadyExists => RepositoryError::User,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Copy, Clone, Debug, PartialEq)]
    struct Dummy(u8);

    #[test]
    fn init_add_has_list_testing() {
        let dummy_one = Dummy(1);
        let repo = MemoryRepository::new(&[dummy_one], Arc::new(|lhs, rhs| lhs.0 == rhs.0));

        // get a list of all records should return 1
        assert_eq!(
            1,
            repo.list(10, 1, |x| Some(*x))
                .expect("No error should happen here")
                .len()
        );
        // and that it exist
        assert_eq!(true, repo.has(&dummy_one).expect("has shouldn't fail here"));

        let dummy_two = Dummy(2);

        // check if a non-existing record returns false
        assert_eq!(
            false,
            repo.has(&dummy_two).expect("has shouldn't fail here")
        );

        assert_eq!(
            Ok(()),
            repo.add(dummy_two),
            "Adding new record should succeed"
        );

        assert_eq!(
            MemoryRepositoryError::AlreadyExists,
            repo.add(dummy_two)
                .expect_err("Adding the same record again should fail")
        );
    }

    #[test]
    fn listing_multiple_pages_and_filtering() {
        let dummy_filter = |x: &Dummy| Some(*x);
        let dummy_one = Dummy(1);
        let dummy_two = Dummy(2);
        let repo =
            MemoryRepository::new(&[dummy_one, dummy_two], Arc::new(|lhs, rhs| lhs.0 == rhs.0));

        // get a list with limit 10 should return 2 records
        assert_eq!(
            2,
            repo.list(10, 1, dummy_filter)
                .expect("No error should happen here")
                .len()
        );

        // get a list with limit 1 and page 1 should return Dummy 1
        let dummy_one_result = repo
            .list(1, 1, dummy_filter)
            .expect("No error should happen here");
        assert_eq!(1, dummy_one_result.len());
        assert_eq!(dummy_one, dummy_one_result[0]);

        // get a list with limit 1 and page 2 should return Dummy 2
        let dummy_two_result = repo
            .list(1, 2, dummy_filter)
            .expect("No error should happen here");
        assert_eq!(1, dummy_two_result.len());
        assert_eq!(dummy_two, dummy_two_result[0]);

        // get a list filtering out Dummy > 2
        repo.add(Dummy(3)).expect("The Dummy(3) should be added");

        assert_eq!(3, repo.list(10, 1, dummy_filter).unwrap().len());

        let filtered_result = repo
            .list(10, 1, |x| if x.0 > 2 { None } else { Some(*x) })
            .expect("No error should happen here");

        assert_eq!(vec![dummy_one, dummy_two], filtered_result);
    }
}
