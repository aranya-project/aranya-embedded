#![cfg(feature = "storage-internal")]

use core::cell::RefCell;

use alloc::boxed::Box;
use alloc::sync::Arc;

use aranya_runtime::linear::LinearStorageProvider;
use aranya_runtime::storage::linear::io;
use aranya_runtime::{GraphId, Location, StorageError as AranyaStorageError};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use esp_partition_table::{DataPartitionType, PartitionEntry, PartitionTable, PartitionType};
use esp_storage::FlashStorage;
use rkyv::rancor;

use super::StorageError;

pub type VolumeMan = ();

#[derive(Clone, PartialEq, rkyv::Serialize, rkyv::Deserialize, rkyv::Archive)]
struct EspStorageHeader {
    epoch: u32,
    head: Option<(u32, u32)>,
    stored_bytes: usize,
}
const HEADER_MAGIC: [u8; 4] = [0x1C, 0x53, 0x4F, 0x00];
const HEADER_SIZE: usize = size_of::<ArchivedEspStorageHeader>();
const DATA_OFFSET: u32 = FlashStorage::SECTOR_SIZE;

#[derive(rkyv::Serialize, rkyv::Archive)]
struct SegmentHeader {
    size: u32,
}
const SEGMENT_HEADER_SIZE: usize = size_of::<ArchivedSegmentHeader>();
const SEGMENT_HEADER_MAGIC: [u8; 4] = [0x1E, 0x53, 0x4F, 0x00];

fn find_data_partition(flash: &mut FlashStorage) -> Result<PartitionEntry, StorageError> {
    let table = PartitionTable::new(PartitionTable::DEFAULT_ADDR, PartitionTable::MAX_SIZE);
    let mut entries = table.iter_storage(flash, true);
    let mut data_partition = None;

    for p in &mut entries {
        if let Ok(p) = p {
            log::info!("{}: at {:X} size {:X}", p.name(), p.offset, p.size);
            if matches!(p.type_, PartitionType::Data(DataPartitionType::LittleFS)) {
                data_partition = Some(p);
            }
        }
    }

    if let Some(true) = entries.check_md5() {
        log::debug!("Partition table checksum OK");
    } else {
        log::error!(
            "Partition table MD5 checksum does not match: {:?} calculated, {:?} stored",
            entries.actual_md5(),
            entries.stored_md5()
        );
    }

    data_partition.ok_or(StorageError::NoDataPartitionFound)
}

fn fetch_header<S>(
    storage: &Arc<Mutex<CriticalSectionRawMutex, RefCell<S>>>,
    offset: u32,
) -> Result<EspStorageHeader, StorageError>
where
    S: embedded_storage::ReadStorage,
{
    let mut buf = [0u8; HEADER_MAGIC.len() + HEADER_SIZE];
    storage.lock(|storage| storage.borrow_mut().read(offset, &mut buf));
    if buf[0..HEADER_MAGIC.len()] != HEADER_MAGIC {
        return Err(StorageError::BadHeader);
    }
    log::info!("magic OK");
    let header =
        rkyv::access::<ArchivedEspStorageHeader, rancor::Error>(&buf[HEADER_MAGIC.len()..])
            .map_err(|_| StorageError::BadHeader)?;
    let header = rkyv::deserialize::<EspStorageHeader, rancor::Error>(header)
        .map_err(|_| StorageError::BadHeader)?;
    Ok(header)
}

fn write_header<S>(
    storage: &Arc<Mutex<CriticalSectionRawMutex, RefCell<S>>>,
    header: &EspStorageHeader,
    offset: u32,
) -> Result<(), StorageError>
where
    S: embedded_storage::Storage,
{
    let bytes = rkyv::to_bytes::<rancor::Error>(header)?;

    storage
        .lock(|storage| storage.borrow_mut().write(offset, &bytes))
        .map_err(|_| StorageError::Write)?;

    Ok(())
}

pub fn init() -> Result<LinearStorageProvider<EspPartitionIoManager<FlashStorage>>, StorageError> {
    log::info!("Initialize Internal Storage");
    let mut storage = FlashStorage::new();
    let data_partition = find_data_partition(&mut storage)?;

    Ok(LinearStorageProvider::new(EspPartitionIoManager::new(
        storage,
        data_partition,
    )))
}

pub struct Reader<S>
where
    S: embedded_storage::ReadStorage,
{
    base: u32,
    storage: Arc<Mutex<CriticalSectionRawMutex, RefCell<S>>>,
}

impl<S> Clone for Reader<S>
where
    S: embedded_storage::ReadStorage,
{
    fn clone(&self) -> Self {
        Self {
            base: self.base,
            storage: Arc::clone(&self.storage),
        }
    }
}

impl<S> io::Read for Reader<S>
where
    S: embedded_storage::ReadStorage,
{
    fn fetch<T>(&self, offset: usize) -> Result<T, AranyaStorageError>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut segment_header = [0u8; SEGMENT_HEADER_MAGIC.len() + SEGMENT_HEADER_SIZE];
        let offset: u32 = offset.try_into().map_err(|_| AranyaStorageError::IoError)?;

        self.storage
            .lock(|s| s.borrow_mut().read(self.base + offset, &mut segment_header))
            .map_err(|_| AranyaStorageError::IoError)?;
        if segment_header[0..SEGMENT_HEADER_MAGIC.len()] != SEGMENT_HEADER_MAGIC {
            return Err(AranyaStorageError::IoError);
        }

        let header = rkyv::access::<ArchivedSegmentHeader, rancor::Error>(
            &segment_header[SEGMENT_HEADER_MAGIC.len()..],
        )
        .map_err(|_| AranyaStorageError::IoError)?;
        let data_size = header
            .size
            .try_into()
            .map_err(|_| AranyaStorageError::IoError)?;
        let byte_buf = Box::new_uninit_slice(data_size);
        // SAFETY: uhhhhhhhhh
        let mut byte_buf = unsafe { byte_buf.assume_init() };
        self.storage
            .lock(|s| {
                s.borrow_mut().read(
                    self.base + offset + SEGMENT_HEADER_SIZE as u32,
                    &mut byte_buf,
                )
            })
            .map_err(|_| AranyaStorageError::IoError)?;
        postcard::from_bytes(&byte_buf).map_err(|_| AranyaStorageError::IoError)
    }
}

pub struct Writer<S>
where
    S: embedded_storage::Storage,
{
    base: u32,
    header_cache: EspStorageHeader,
    storage: Arc<Mutex<CriticalSectionRawMutex, RefCell<S>>>,
}

impl<S> Writer<S>
where
    S: embedded_storage::Storage,
{
    fn new(storage: Arc<Mutex<CriticalSectionRawMutex, RefCell<S>>>, base: u32) -> Writer<S> {
        let header = fetch_header(&storage, base).expect("could not fetch header");
        Writer {
            base,
            header_cache: header,
            storage,
        }
    }

    fn update_header<F>(&mut self, update: F) -> Result<(), StorageError>
    where
        F: FnOnce(&mut EspStorageHeader) -> Result<(), StorageError>,
    {
        let mut header = self.header_cache.clone();
        header.epoch += 1;
        update(&mut header)?;
        write_header(&self.storage, &header, self.base)?;

        self.header_cache = fetch_header(&self.storage, self.base)?;
        if self.header_cache != header {
            log::error!("header write failed to read back correct data");
            return Err(StorageError::Write);
        }
        Ok(())
    }
}

impl<S> io::Write for Writer<S>
where
    S: embedded_storage::Storage,
{
    type ReadOnly = Reader<S>;

    fn readonly(&self) -> Self::ReadOnly {
        Reader {
            base: self.base,
            storage: Arc::clone(&self.storage),
        }
    }

    fn head(&self) -> Result<Location, AranyaStorageError> {
        self.header_cache
            .head
            .map(|(a, b)| Ok(Location::new(a as usize, b as usize)))
            .ok_or_else(|| AranyaStorageError::NoSuchStorage)?
    }

    fn append<F, T>(&mut self, builder: F) -> Result<T, AranyaStorageError>
    where
        F: FnOnce(usize) -> T,
        T: serde::Serialize,
    {
        let offset = self.header_cache.stored_bytes;
        let item = builder(offset);
        let bytes = postcard::to_allocvec(&item).map_err(|_| AranyaStorageError::IoError)?;
        let write_pos = self.header_cache.stored_bytes as u32;
        self.storage
            .lock(|storage| {
                storage
                    .borrow_mut()
                    .write(self.base + DATA_OFFSET + write_pos, &bytes)
            })
            .map_err(|_| AranyaStorageError::IoError)?;

        self.update_header(|header| {
            header.stored_bytes += bytes.len();
            Ok(())
        })
        .map_err(|_| AranyaStorageError::IoError)?;

        Ok(item)
    }

    fn commit(&mut self, head: Location) -> Result<(), AranyaStorageError> {
        self.update_header(|header| {
            let segment = head
                .segment
                .try_into()
                .map_err(|_| AranyaStorageError::IoError)?;
            let command = head
                .segment
                .try_into()
                .map_err(|_| AranyaStorageError::IoError)?;
            header.head = Some((segment, command));
            Ok(())
        })
        .map_err(|_| AranyaStorageError::IoError)?;

        Ok(())
    }
}

pub struct EspPartitionIoManager<S>
where
    S: embedded_storage::Storage,
{
    storage: Arc<Mutex<CriticalSectionRawMutex, RefCell<S>>>,
    base: u32,
    size: usize,
}

impl<S> EspPartitionIoManager<S>
where
    S: embedded_storage::Storage,
{
    pub fn new(storage: S, partition: PartitionEntry) -> EspPartitionIoManager<S>
    where
        <S as embedded_storage::ReadStorage>::Error: core::fmt::Debug,
    {
        let storage = Arc::new(Mutex::new(RefCell::new(storage)));
        match fetch_header(&storage, partition.offset) {
            Ok(_) => (),
            Err(StorageError::BadHeader) => {
                log::info!("header bad; initializing storage");
                let header = EspStorageHeader {
                    epoch: 0,
                    head: None,
                    stored_bytes: 0,
                };
                write_header(&storage, &header, partition.offset)
                    .expect("could not write header");
            }
            Err(e) => {
                log::error!("{e}");
            }
        }
        EspPartitionIoManager {
            storage,
            base: partition.offset,
            size: partition.size,
        }
    }
}

impl<S> io::IoManager for EspPartitionIoManager<S>
where
    S: embedded_storage::Storage,
{
    type Writer = Writer<S>;

    fn create(&mut self, _id: GraphId) -> Result<Self::Writer, AranyaStorageError> {
        Ok(Writer::new(Arc::clone(&self.storage), self.base))
    }

    fn open(&mut self, _id: GraphId) -> Result<Option<Self::Writer>, AranyaStorageError> {
        Ok(None)
    }
}
