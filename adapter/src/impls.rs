use std::ops::{Deref, DerefMut};
use super::*;

impl<T: LockedClient> Deref for Locked<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: LockedClient> DerefMut for Locked<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: UnlockedClient> Deref for Unlocked<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: UnlockedClient> DerefMut for Unlocked<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}