mod embedded;
mod file;

#[cfg(feature = "embedded")]
pub use embedded::*;

#[cfg(feature = "std")]
pub use file::*;

use crate::ParameterStoreError;

// AbstractIO is a very basic wrapper for underlying IO implementations. It is an
// all-or-nothing read/write interface - there is no seeking or partial transfers.
pub trait AbstractIO {
    fn write(&mut self, buf: &[u8]) -> Result<(), ParameterStoreError>;

    fn read(&mut self, buf: &mut [u8]) -> Result<(), ParameterStoreError>;
}
