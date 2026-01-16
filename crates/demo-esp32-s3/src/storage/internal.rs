#![cfg(feature = "storage-internal")]

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::cell::RefCell;

use aranya_runtime::{
    linear::LinearStorageProvider, storage::linear::io, GraphId, Location,
    StorageError as AranyaStorageError,
};
use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};
use embedded_storage::Storage;
use esp_partition_table::{DataPartitionType, PartitionEntry, PartitionTable, PartitionType};
use esp_storage::FlashStorage;
use rkyv::rancor;

use super::StorageError;

#[derive(Clone, PartialEq, rkyv::Serialize, rkyv::Deserialize, rkyv::Archive)]
struct EspStorageHeader {
    epoch: u32,
    graph_id: Option<GraphId>,
    head: Option<(u32, u32)>,
    stored_bytes: usize,
}

const MAGIC_LEN: usize = 4;
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

    for p in (&mut entries).flatten() {
        log::debug!("{}: at {:X} size {:X}", p.name(), p.offset, p.size);
        if matches!(p.type_, PartitionType::Data(DataPartitionType::Undefined)) && p.name == "graph"
        {
            data_partition = Some(p);
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
    let mut buf = rkyv::util::Align([0u8; HEADER_MAGIC.len() + HEADER_SIZE]);
    storage
        .lock(|storage| storage.borrow_mut().read(offset, buf.as_mut()))
        .map_err(|_| StorageError::BadHeader)?;
    if buf[0..HEADER_MAGIC.len()] != HEADER_MAGIC {
        return Err(StorageError::BadHeader);
    }
    log::debug!("magic OK");
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
    let mut buf = Vec::with_capacity(HEADER_MAGIC.len() + HEADER_SIZE);
    buf.extend_from_slice(&HEADER_MAGIC);
    buf.extend_from_slice(&rkyv::to_bytes::<rancor::Error>(header)?);

    log::debug!("write header @{offset:08X}");
    storage
        .lock(|storage| storage.borrow_mut().write(offset, &buf))
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

// Destroy this storage by erasing the header block
pub fn nuke() -> Result<(), StorageError> {
    let mut storage = FlashStorage::new();
    let data_partition = find_data_partition(&mut storage)?;
    storage
        .write(
            data_partition.offset,
            &[0u8; FlashStorage::SECTOR_SIZE as usize],
        )
        .map_err(storage_error)?;
    Ok(())
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

fn log_error<E>(se: AranyaStorageError) -> impl FnOnce(E) -> AranyaStorageError
where
    E: core::fmt::Display,
{
    |e: E| {
        log::error!("{e}");
        se
    }
}

// Unfortunately esp_storage::FlashStorage does not implement Display, so this is all we can do.
fn storage_error<E>(e: E) -> AranyaStorageError
where
    E: alloc::fmt::Debug,
{
    log::error!("Storage error: {e:?}");
    AranyaStorageError::IoError
}

impl<S> io::Read for Reader<S>
where
    S: embedded_storage::ReadStorage,
    <S as embedded_storage::ReadStorage>::Error: core::fmt::Debug,
{
    fn fetch<T>(&self, offset: usize) -> Result<T, AranyaStorageError>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut segment_header = rkyv::util::Align([0u8; MAGIC_LEN + SEGMENT_HEADER_SIZE]);
        let offset: u32 = offset
            .try_into()
            .map_err(log_error(AranyaStorageError::IoError))?;

        let read_pos = self.base + DATA_OFFSET + offset;
        self.storage
            .lock(|s| s.borrow_mut().read(read_pos, segment_header.as_mut()))
            .map_err(storage_error)?;
        if segment_header[0..MAGIC_LEN] != SEGMENT_HEADER_MAGIC {
            log::error!(
                "bad segment header magic: {:?} != {:?}",
                &segment_header[0..MAGIC_LEN],
                SEGMENT_HEADER_MAGIC
            );
            return Err(AranyaStorageError::IoError);
        }

        let header =
            rkyv::access::<ArchivedSegmentHeader, rancor::Error>(&segment_header[MAGIC_LEN..])
                .map_err(log_error(AranyaStorageError::IoError))?;
        let data_size = header
            .size
            .try_into()
            .map_err(log_error(AranyaStorageError::IoError))?;

        log::debug!("Fetching segment @ {offset}, len {data_size}");
        log::trace!("  header bytes: {:?}", &segment_header);
        // SAFETY: the box is zeroed before we `assume_init()`
        let mut byte_buf = unsafe { Box::new_zeroed_slice(data_size).assume_init() };
        let read_pos = read_pos + (MAGIC_LEN + SEGMENT_HEADER_SIZE) as u32;
        self.storage
            .lock(|s| s.borrow_mut().read(read_pos, &mut byte_buf))
            .map_err(storage_error)?;
        log::trace!("  {} data bytes: {:?}", byte_buf.len(), &byte_buf);
        postcard::from_bytes(&byte_buf).map_err(log_error(AranyaStorageError::IoError))
    }
}

pub struct Writer<S>
where
    S: embedded_storage::Storage,
{
    base: u32,
    size: usize,
    header_cache: EspStorageHeader,
    storage: Arc<Mutex<CriticalSectionRawMutex, RefCell<S>>>,
}

impl<S> Writer<S>
where
    S: embedded_storage::Storage,
{
    fn new(
        storage: Arc<Mutex<CriticalSectionRawMutex, RefCell<S>>>,
        base: u32,
        size: usize,
    ) -> Result<Writer<S>, AranyaStorageError> {
        let header =
            fetch_header(&storage, base).map_err(log_error(AranyaStorageError::IoError))?;
        Self::new_with_header(storage, base, size, header)
    }

    fn new_with_header(
        storage: Arc<Mutex<CriticalSectionRawMutex, RefCell<S>>>,
        base: u32,
        size: usize,
        header: EspStorageHeader,
    ) -> Result<Writer<S>, AranyaStorageError> {
        Ok(Writer {
            base,
            size,
            header_cache: header,
            storage,
        })
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
    <S as embedded_storage::ReadStorage>::Error: core::fmt::Debug,
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
            .ok_or_else(|| {
                log::error!("no head found");
                AranyaStorageError::NoSuchStorage
            })?
    }

    fn append<F, T>(&mut self, builder: F) -> Result<T, AranyaStorageError>
    where
        F: FnOnce(usize) -> T,
        T: serde::Serialize,
    {
        let offset = self.header_cache.stored_bytes;
        let item = builder(offset);
        let mut item_bytes =
            postcard::to_allocvec(&item).map_err(log_error(AranyaStorageError::IoError))?;
        log::debug!("Appending segment @ {offset}, len {}", item_bytes.len());
        let item_size = SEGMENT_HEADER_MAGIC.len() + SEGMENT_HEADER_SIZE + item_bytes.len();
        if self.header_cache.stored_bytes + item_size > self.size {
            log::error!("Internal storage out of space");
            return Err(AranyaStorageError::IoError);
        }

        let mut disk_bytes = SEGMENT_HEADER_MAGIC.to_vec();
        disk_bytes.extend_from_slice(
            &rkyv::to_bytes::<rancor::Error>(&SegmentHeader {
                // This should never panic as it is exceedingly unlikely that the segment size
                // won't fit in a `u32`.
                size: item_bytes.len().try_into().unwrap(),
            })
            .map_err(log_error(AranyaStorageError::IoError))?,
        );
        log::trace!("  header bytes: {:?}", &disk_bytes);
        log::trace!("  {} data bytes: {:?}", item_bytes.len(), &item_bytes);

        disk_bytes.append(&mut item_bytes);
        assert_eq!(disk_bytes.len(), item_size);

        let write_pos = self.base + DATA_OFFSET + offset as u32;
        log::debug!("write segment @{write_pos:08X}");
        self.storage
            .lock(|storage| storage.borrow_mut().write(write_pos, &disk_bytes))
            .map_err(storage_error)?;

        self.update_header(|header| {
            header.stored_bytes += item_size;
            Ok(())
        })
        .map_err(log_error(AranyaStorageError::IoError))?;

        Ok(item)
    }

    fn commit(&mut self, head: Location) -> Result<(), AranyaStorageError> {
        log::debug!("commit {head}");
        self.update_header(|header| {
            let segment = head
                .segment
                .try_into()
                .map_err(log_error(AranyaStorageError::IoError))?;
            let command = head
                .command
                .try_into()
                .map_err(log_error(AranyaStorageError::IoError))?;
            header.head = Some((segment, command));
            Ok(())
        })
        .map_err(log_error(AranyaStorageError::IoError))?;

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
                    graph_id: None,
                    head: None,
                    stored_bytes: 0,
                };
                write_header(&storage, &header, partition.offset).expect("could not write header");
            }
            Err(e) => {
                log::error!("{e}");
            }
        }

        log::info!(
            "Graph partition is at {:X} size {:X}",
            partition.offset,
            partition.size
        );

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
    <S as embedded_storage::ReadStorage>::Error: core::fmt::Debug,
{
    type Writer = Writer<S>;

    fn create(&mut self, id: GraphId) -> Result<Self::Writer, AranyaStorageError> {
        let header = fetch_header(&self.storage, self.base)
            .map_err(log_error(AranyaStorageError::NoSuchStorage))?;
        if header.graph_id.is_some() {
            return Err(AranyaStorageError::StorageExists);
        }
        let mut writer = Writer::new(Arc::clone(&self.storage), self.base, self.size)?;
        writer
            .update_header(|h| {
                h.graph_id = Some(id.into());
                Ok(())
            })
            .map_err(log_error(AranyaStorageError::IoError))?;

        Ok(writer)
    }

    fn open(&mut self, id: GraphId) -> Result<Option<Self::Writer>, AranyaStorageError> {
        let header = fetch_header(&self.storage, self.base)
            .map_err(log_error(AranyaStorageError::NoSuchStorage))?;

        if let Some(graph_id) = header.graph_id {
            if graph_id != id {
                log::error!("wrong graph ID");
                return Err(AranyaStorageError::NoSuchStorage);
            }
        }

        Ok(Some(Writer::new_with_header(
            Arc::clone(&self.storage),
            self.base,
            self.size,
            header,
        )?))
    }

    fn list(
        &mut self,
    ) -> Result<impl Iterator<Item = Result<GraphId, AranyaStorageError>>, AranyaStorageError> {
        let header = fetch_header(&self.storage, self.base)
            .map_err(log_error(AranyaStorageError::NoSuchStorage))?;
        Ok(header.graph_id.into_iter().map(Ok))
    }

    fn remove(&mut self, id: GraphId) -> Result<(), AranyaStorageError> {
        unimplemented!()
    }
}
