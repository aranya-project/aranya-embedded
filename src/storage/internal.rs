#![cfg(feature = "storage-internal")]

use core::cell::RefCell;

use alloc::sync::Arc;

use aranya_runtime::storage::linear::io;
use aranya_runtime::{GraphId, Location, StorageError as AranyaStorageError};
use embassy_sync::blocking_mutex::{raw::NoopRawMutex, Mutex};
use esp_partition_table::{DataPartitionType, PartitionEntry, PartitionTable, PartitionType};
use esp_storage::FlashStorage;

use super::StorageError;

pub type VolumeMan = ();

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

pub fn init() -> Result<LinearStorageProvider<EspPartitionIoManager<FlashStorage>>, StorageError> {
    log::info!("Initialize Internal Storage");
    let mut storage = FlashStorage::new();
    let data_partition = find_data_partition(&mut storage)?;

    Ok(LinearStorageProvider::new(EspPartitionIoManager::new(storage, data_partition)))
}

struct EspStorageHeader {
    epoch: u32,
    head: Option<Location>,
    stored_bytes: usize,
}
const HEADER_MAGIC: [u8; 4] = [0x55, 0xAA, 0x41, 0xDE];
const HEADER_SIZE: usize = 21;

pub struct EspPartitionIoManager<S>
where
    S: embedded_storage::Storage,
{
    storage: Arc<Mutex<NoopRawMutex, RefCell<S>>>,
    offset: u32,
    size: usize,
    epoch: u32,
    head: Option<Location>,
    stored_bytes: usize,
}

impl<S> EspPartitionIoManager<S>
where
    S: embedded_storage::Storage,
{
    pub fn new(mut storage: S, partition: PartitionEntry) -> EspPartitionIoManager<S>
    where
        <S as embedded_storage::ReadStorage>::Error: core::fmt::Debug,
    {
        let header = Self::fetch_header(&mut storage, partition.offset);
        if let Some(header) = header {
            EspPartitionIoManager {
                storage: Arc::new(Mutex::new(RefCell::new(storage))),
                offset: partition.offset,
                size: partition.size,
                epoch: header.epoch,
                head: header.head,
                stored_bytes: header.stored_bytes,
            }
        } else {
            log::info!("header bad; initializing storage");
            let mut iom = EspPartitionIoManager {
                storage: Arc::new(Mutex::new(RefCell::new(storage))),
                offset: partition.offset,
                size: partition.size,
                epoch: 0,
                head: None,
                stored_bytes: 0,
            };
            iom.write_header().expect("could not write header");
            iom
        }
    }

    fn fetch_header(storage: &mut S, offset: u32) -> Option<EspStorageHeader>
    where
        <S as embedded_storage::ReadStorage>::Error: core::fmt::Debug,
    {
        let mut buf = [0u8; HEADER_SIZE];
        storage.read(offset, &mut buf);
        log::info!("header: {:X?}", buf);
        if buf[0..4] == HEADER_MAGIC {
            log::info!("magic OK");
            let epoch = u32::from_le_bytes((&buf[4..8]).try_into().unwrap());
            let head_valid = buf[8] != 0;
            let head = if head_valid {
                let segment = u32::from_le_bytes((&buf[9..13]).try_into().unwrap());
                let command = u32::from_le_bytes((&buf[13..17]).try_into().unwrap());
                Some(Location::new(segment as usize, command as usize))
            } else {
                None
            };
            let stored_bytes = u32::from_le_bytes((&buf[17..21]).try_into().unwrap()) as usize;
            Some(EspStorageHeader {
                epoch,
                head,
                stored_bytes,
            })
        } else {
            None
        }
    }

    fn write_header(&mut self) -> Result<(), StorageError>
    where
        <S as embedded_storage::ReadStorage>::Error: core::fmt::Debug,
    {
        let mut buf = [0u8; HEADER_SIZE];

        self.epoch += 1;

        buf[0..4].copy_from_slice(&HEADER_MAGIC);
        buf[4..8].copy_from_slice(&self.epoch.to_le_bytes());
        if let Some(head) = self.head {
            buf[8] = 1;
            let segment: u32 = head.segment.try_into().unwrap();
            let command: u32 = head.command.try_into().unwrap();
            buf[9..13].copy_from_slice(&segment.to_le_bytes());
            buf[13..17].copy_from_slice(&command.to_le_bytes())
        }
        let stored_bytes: u32 = self.stored_bytes.try_into().unwrap();
        buf[17..21].copy_from_slice(&stored_bytes.to_le_bytes());

        self.storage
            .lock(|s| s.borrow_mut().write(self.offset, &buf).expect("write"));

        Ok(())
    }
}

/* impl<S> io::IoManager for EspPartitionIoManager<S>
where
    S: embedded_storage::Storage + Clone,
{
    type Writer = Writer<S>;

    fn create(&mut self, _id: GraphId) -> Result<Self::Writer, AranyaStorageError> {
        Ok(Writer {
            head: Mutex::new(None),
            storage: Arc::clone(&self.storage),
        })
    }

    fn open(&mut self, _id: GraphId) -> Result<Option<Self::Writer>, AranyaStorageError> {
        Ok(None)
    }
}

pub struct Writer<S>
where
    S: embedded_storage::Storage,
{
    head: Mutex<NoopRawMutex, Option<Location>>,
    storage: Arc<Mutex<NoopRawMutex, S>>,
}

#[derive(Clone)]
pub struct Reader<S>
where
    S: embedded_storage::Storage,
{
    storage: Arc<Mutex<NoopRawMutex, S>>,
}

impl<S> io::Write for Writer<S>
where
    S: embedded_storage::Storage + Clone,
{
    type ReadOnly = Reader<S>;

    fn readonly(&self) -> Self::ReadOnly {
        Reader {
            storage: Arc::clone(&self.storage),
        }
    }

    fn head(&self) -> Result<Location, AranyaStorageError> {
        let head = self.head.lock(|v| *v);
        Ok(head.ok_or(AranyaStorageError::NoSuchStorage)?)
    }

    fn append<F, T>(&mut self, builder: F) -> Result<T, AranyaStorageError>
    where
        F: FnOnce(usize) -> T,
        T: serde::Serialize,
    {
        let offset = self.shared.items.lock().len();
        let item = builder(offset);
        let bytes = postcard::to_allocvec(&item)
            .map_err(|_| StorageError::IoError)?
            .into_boxed_slice();
        self.shared.items.lock().push(bytes);
        Ok(item)
    }

    fn commit(&mut self, head: Location) -> Result<(), AranyaStorageError> {
        self.head.lock(|v| *v = Some(head));
        Ok(())
    }
}

impl<S> io::Read for Reader<S>
where
    S: embedded_storage::Storage + Clone,
{
    fn fetch<T>(&self, offset: usize) -> Result<T, AranyaStorageError>
    where
        T: serde::de::DeserializeOwned,
    {
        let items = self.storage.items.lock();
        let bytes = items
            .get(offset)
            .ok_or(StorageError::SegmentOutOfBounds(Location::new(offset, 0)))?;
        postcard::from_bytes(bytes).map_err(|_| StorageError::IoError)
    }
}
 */
