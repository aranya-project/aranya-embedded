#![cfg(feature = "embedded")]

use super::AbstractIO;
use crate::ParameterStoreError;

pub struct EmbeddedStorageIO<S> {
    storage: S,
    offset: u32,
}

impl<S> EmbeddedStorageIO<S> {
    pub fn new(storage: S, offset: u32) -> EmbeddedStorageIO<S> {
        EmbeddedStorageIO { storage, offset }
    }
}

impl<S> AbstractIO for EmbeddedStorageIO<S>
where
    S: embedded_storage::Storage,
{
    fn write(&mut self, buf: &[u8]) -> Result<(), ParameterStoreError> {
        self.storage
            .write(self.offset, buf)
            .map_err(|_| ParameterStoreError::IO)?;
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<(), ParameterStoreError> {
        self.storage
            .read(self.offset, buf)
            .map_err(|_| ParameterStoreError::IO)?;
        Ok(())
    }
}
