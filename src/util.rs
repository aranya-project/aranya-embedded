// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
#[macro_export]
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

/// SliceCursor is kind of the read-side companion to [`BorrowedCursor`](core::io::BorrowedCursor).
///
/// It will panic if you attempt to read beyond the end of the slice.
pub(crate) struct SliceCursor<'a> {
    slice: &'a [u8],
    pos: usize,
}

impl<'a> SliceCursor<'a> {
    pub fn new(slice: &'a [u8]) -> SliceCursor<'a> {
        SliceCursor { slice, pos: 0 }
    }

    /// Return the number of bytes remaining in the cursor
    pub fn remaining(&self) -> usize {
        self.slice.len() - self.pos
    }

    /// Get a subslice for the next `n` bytes of the slice.
    pub fn next(&mut self, n: usize) -> &[u8] {
        assert!(self.pos + n <= self.slice.len());
        let slice = &self.slice[self.pos..self.pos + n];
        self.pos += n;
        slice
    }

    /// Grab the next byte and return it as a `u8`.
    pub fn next_u8(&mut self) -> u8 {
        self.next(1)[0]
    }

    /// Grab the next two bytes and interpret them as a big-endian `u16`.
    pub fn next_u16_be(&mut self) -> u16 {
        u16::from_be_bytes(self.next(2).try_into().unwrap())
    }
}
