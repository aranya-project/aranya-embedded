use core::marker::PhantomData;

use aranya_crypto::keystore::{Entry, Occupied, Vacant};
use aranya_crypto::KeyStore;
use aranya_crypto::engine::WrappedKey;

use crate::error::Error;

pub struct EmbeddedStore;

pub struct VacantEntry<T> {
    _t: PhantomData<T>,
}

impl<T: WrappedKey> Vacant<T> for VacantEntry<T> {
    type Error = Error;

    fn insert(self, key: T) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub struct OccupiedEntry<T> {
    _t: PhantomData<T>,
}

impl<T: WrappedKey> Occupied<T> for OccupiedEntry<T> {
    type Error = Error;

    fn get(&self) -> Result<T, Self::Error> {
        Err(Error::Other)
    }

    fn remove(self) -> Result<T, Self::Error> {
        Err(Error::Other)
    }
}

impl KeyStore for EmbeddedStore {
    type Error = Error;
    type Vacant<'a, T: WrappedKey> = VacantEntry<T>;
    type Occupied<'a, T: WrappedKey> = OccupiedEntry<T>;

    fn entry<T: WrappedKey>(&mut self, id: aranya_crypto::Id) -> Result<Entry<'_, Self, T>, Self::Error> {
        Err(Error::Other)
    }

    fn get<T: WrappedKey>(&self, id: aranya_crypto::Id) -> Result<Option<T>, Self::Error> {
        Err(Error::Other)
    }
}