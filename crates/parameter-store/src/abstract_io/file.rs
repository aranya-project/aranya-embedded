#![cfg(feature = "std")]

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
};

use super::AbstractIO;
use crate::ParameterStoreError;

pub struct FileIO {
    f: File,
}

impl FileIO {
    pub fn new(f: File) -> FileIO {
        FileIO { f }
    }
}

impl AbstractIO for FileIO {
    fn write(&mut self, buf: &[u8]) -> Result<(), ParameterStoreError> {
        self.f.seek(SeekFrom::Start(0))?;
        self.f.write_all(buf)?;
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<(), ParameterStoreError> {
        self.f.seek(SeekFrom::Start(0))?;
        self.f.read_exact(buf)?;
        Ok(())
    }
}

impl From<std::io::Error> for ParameterStoreError {
    fn from(_value: std::io::Error) -> Self {
        ParameterStoreError::IO
    }
}
