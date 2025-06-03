use core::marker::PhantomData;

use crc::Crc;

use crate::abstract_io::AbstractIO;

const PARAMETER_BLOCK_SIZE: usize = 1024;

#[derive(Debug, thiserror::Error)]
pub enum ParameterStoreError {
    #[error("IO Error")]
    IO,
    #[error("Postcard Error: {0}")]
    Postcard(#[from] postcard::Error),
    #[error("Parameters too large")]
    Size,
    #[error("invalid checksum")]
    Corrupt,
}

pub struct ParameterStore<T, IO> {
    io: IO,
    _pd: PhantomData<T>,
}

impl<T, IO> ParameterStore<T, IO> {
    pub fn new(io: IO) -> ParameterStore<T, IO> {
        ParameterStore::<T, IO> {
            io,
            _pd: PhantomData,
        }
    }
}

impl<T, IO> ParameterStore<T, IO>
where
    T: Sized + serde::Serialize + for<'a> serde::Deserialize<'a>,
    IO: AbstractIO,
{
    pub fn store(&mut self, v: &T) -> Result<T, ParameterStoreError> {
        let mut buffer: heapless::Vec<u8, PARAMETER_BLOCK_SIZE> = heapless::Vec::new();
        let serialized_bytes = postcard::to_allocvec(v)?;
        if serialized_bytes.len() + 8 > PARAMETER_BLOCK_SIZE {
            return Err(ParameterStoreError::Size);
        }

        buffer
            .extend_from_slice(&(serialized_bytes.len() as u32).to_be_bytes())
            .map_err(|_| ParameterStoreError::Size)?;
        buffer
            .extend_from_slice(&serialized_bytes)
            .map_err(|_| ParameterStoreError::Size)?;
        let checksum = Crc::<u32>::new(&crc::CRC_32_CKSUM)
            .checksum(&buffer[..serialized_bytes.len() + 4])
            .to_be_bytes();
        buffer
            .extend_from_slice(&checksum)
            .map_err(|_| ParameterStoreError::Size)?;
        // SAFETY: PARAMETER_BLOCK_SIZE is the size we initialized buffer with
        buffer.resize(PARAMETER_BLOCK_SIZE, 0).unwrap();
        self.io.write(&buffer)?;
        // This seems slightly pointless but we do want to verify that the write was
        // successful, so we re-fetch.
        Ok(self.fetch()?)
    }

    pub fn fetch(&mut self) -> Result<T, ParameterStoreError> {
        let mut buffer = [0u8; PARAMETER_BLOCK_SIZE];
        self.io
            .read(&mut buffer)
            .map_err(|_| ParameterStoreError::IO)?;
        let data_size = u32::from_be_bytes(buffer[0..4].try_into().unwrap()) as usize;
        if data_size > PARAMETER_BLOCK_SIZE - 8 {
            return Err(ParameterStoreError::Corrupt);
        }
        let checksum = Crc::<u32>::new(&crc::CRC_32_CKSUM)
            .checksum(&buffer[0..data_size + 4])
            .to_be_bytes();
        if checksum != buffer[data_size + 4..data_size + 8] {
            return Err(ParameterStoreError::Corrupt);
        }
        let v = postcard::from_bytes(&buffer[4..data_size + 4])?;
        Ok(v)
    }

    pub fn update<F>(&mut self, f: F) -> Result<T, ParameterStoreError>
    where
        F: FnOnce(&mut T),
    {
        let mut v = self.fetch()?;
        f(&mut v);
        self.store(&v)?;
        Ok(v)
    }
}
