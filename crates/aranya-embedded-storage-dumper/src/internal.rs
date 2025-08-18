#[derive(Debug, Clone, PartialEq, rkyv::Serialize, rkyv::Deserialize, rkyv::Archive)]
pub struct Id([u8; 32]);

#[derive(Debug, Clone, PartialEq, rkyv::Serialize, rkyv::Deserialize, rkyv::Archive)]
pub struct EspStorageHeader {
    pub epoch: u32,
    pub graph_id: Option<Id>,
    pub head: Option<(u32, u32)>,
    pub stored_bytes: usize,
}

pub const HEADER_MAGIC: [u8; 4] = [0x1C, 0x53, 0x4F, 0x00];
pub const HEADER_SIZE: usize = size_of::<ArchivedEspStorageHeader>();
pub const DATA_OFFSET: usize = 4096;

#[derive(Debug, rkyv::Serialize, rkyv::Deserialize, rkyv::Archive)]
pub struct SegmentHeader {
    pub size: u32,
}

pub const SEGMENT_HEADER_SIZE: usize = size_of::<ArchivedSegmentHeader>();
pub const SEGMENT_HEADER_MAGIC: [u8; 4] = [0x1E, 0x53, 0x4F, 0x00];
