use core::{cmp::Ordering, hash::Hasher};

use alloc::{
    format,
    string::{String, ToString},
    sync::Arc,
};
use esp_println::println;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::hardware::esp32_time::Esp32TimeSource;
use aranya_crypto::id::{String64, ToBase58};
use aranya_runtime::{
    linear::{IoManager, Read, Write},
    GraphId, Location, StorageError,
};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{
    BlockDevice, Directory, File, Mode, RawFile, RawVolume, SdCard, TimeSource, Volume, VolumeIdx,
    VolumeManager,
};
use esp_hal::{
    delay::Delay, gpio::Output, peripheral::Peripheral, peripherals::TIMG1, spi::master::Spi,
    timer::timg::TimerX,
};

type VolumeMan = VolumeManager<
    SdCard<ExclusiveDevice<Spi<'static, esp_hal::Blocking>, Output<'static>, Delay>, Delay>,
    Esp32TimeSource<TimerX<<TIMG1 as Peripheral>::P, 1>>,
    4,
    4,
    1,
>;

/// A file-backed implementation of [`IoManager`].
#[clippy::has_significant_drop]
pub struct FileManager {
    vol: Arc<VolumeMan>,
    id: Option<GraphId>,
}

impl FileManager {
    /*/// Creates a `FileManager` at `dir`.
    pub fn new<P: AsRef<Path>>(dir: P) -> Result<Self, Error> {
        let fd = libc::open(dir.as_ref(), O_RDONLY | O_DIRECTORY | O_CLOEXEC, 0)?;
        Ok(Self {
            fd,
            // TODO(eric): skip the alloc if `P` is `PathBuf`?
            #[cfg(target_os = "vxworks")]
            dir: dir.as_ref().to_path_buf(),
        })
    }*/

    /// Creates a `FileManager` at `dir`.
    pub fn new(volume_mgr: Arc<VolumeMan>) -> Result<Self, StorageError> {
        Ok(Self {
            vol: volume_mgr,
            id: None,
        })
    }
}

impl IoManager for FileManager {
    type Writer = Writer;

    fn create(&mut self, id: GraphId) -> Result<Self::Writer, StorageError> {
        self.id = Some(id);
        let writer = Writer::create(self);
        writer
    }

    fn open(&mut self, id: GraphId) -> Result<Option<Self::Writer>, StorageError> {
        self.id = Some(id);
        let writer = Ok(Some(Writer::open(self)?));
        writer
    }
}

/// A file-based writer for linear storage.
pub struct Writer {
    file: FileHandle,
    root: Root,
}

/// An estimated page size for spacing the control data.
const PAGE: i64 = 4096;

// We store 2 roots for redudancy.
/// Offset of the first [`Root`].
const ROOT_A: i64 = PAGE;
/// Offset of the second [`Root`].
const ROOT_B: i64 = PAGE * 2;

/// Starting offset for segment/fact data
const FREE_START: i64 = PAGE * 3;

impl Writer {
    fn create(file_manager: &mut FileManager) -> Result<Self, StorageError> {
        let raw_vol = file_manager.vol.open_raw_volume(VolumeIdx(0)).unwrap();
        let raw_dir = file_manager.vol.open_root_dir(raw_vol).unwrap();
        let file_name = filename_8_3(file_manager.id.unwrap());
        let raw_file = file_manager
            .vol
            .open_file_in_dir(raw_dir, file_name.as_str(), Mode::ReadWriteCreate)
            .unwrap();

        let file = FileHandle {
            fd: Arc::new(raw_file),
            volume_manager: file_manager.vol.clone(),
        };
        // Preallocate so we can start appending from FREE_START
        // forward.
        // ! This might mean he starts from FREE_START. ASK file.fallocate(0, FREE_START)?;
        Ok(Self {
            file,
            root: Root::new(),
        })
    }

    fn open(file_manager: &mut FileManager) -> Result<Self, StorageError> {
        let raw_vol = file_manager.vol.open_raw_volume(VolumeIdx(0)).unwrap();
        let raw_dir = file_manager.vol.open_root_dir(raw_vol).unwrap();
        let file_name = filename_8_3(file_manager.id.unwrap());
        let raw_file = file_manager
            .vol
            .open_file_in_dir(raw_dir, file_name.as_str(), Mode::ReadWriteCreate)
            .unwrap();
        let file = FileHandle {
            fd: Arc::new(raw_file),
            volume_manager: file_manager.vol.clone(),
        };

        // Pick the latest valid root.
        let (root, overwrite) = match (
            file.load(ROOT_A).and_then(Root::validate),
            file.load(ROOT_B).and_then(Root::validate),
        ) {
            (Ok(root_a), Ok(root_b)) => match root_a.generation.cmp(&root_b.generation) {
                Ordering::Equal => (root_a, None),
                Ordering::Greater => (root_a, Some(ROOT_B)),
                Ordering::Less => (root_b, Some(ROOT_A)),
            },
            (Ok(root_a), Err(_)) => (root_a, Some(ROOT_B)),
            (Err(_), Ok(root_b)) => (root_b, Some(ROOT_A)),
            (Err(e), Err(_)) => return Err(e),
        };

        // Write other side if needed (corrupted or outdated)
        if let Some(offset) = overwrite {
            file.dump(offset, &root)?;
        }

        Ok(Self { file, root })
    }

    fn write_root(&mut self) -> Result<(), StorageError> {
        self.root.generation = self.root.generation.checked_add(1).unwrap();

        // Write roots one at a time, flushing afterward to
        // ensure one is always valid.
        for offset in [ROOT_A, ROOT_B] {
            self.root.checksum = self.root.calc_checksum();
            self.file.dump(offset, &self.root)?;
            self.file.sync()?;
        }

        Ok(())
    }
}

impl Write for Writer {
    type ReadOnly = Reader;
    fn readonly(&self) -> Self::ReadOnly {
        Reader {
            file: self.file.clone(),
        }
    }

    fn head(&self) -> Result<Location, StorageError> {
        if self.root.generation == 0 {
            println!("not initialized")
        }
        Ok(self.root.head)
    }

    fn append<F, T>(&mut self, builder: F) -> Result<T, StorageError>
    where
        F: FnOnce(usize) -> T,
        T: Serialize,
    {
        let offset = self.root.free_offset;

        let item = builder(offset.try_into().unwrap());
        let new_offset = self.file.dump(offset, &item)?;

        self.root.free_offset = new_offset;
        self.write_root()?;

        Ok(item)
    }

    fn commit(&mut self, head: Location) -> Result<(), StorageError> {
        self.root.head = head;
        self.write_root()?;
        Ok(())
    }
}

/// Section of control data for the file
#[derive(Debug, Serialize, Deserialize)]
struct Root {
    /// Incremented each commit
    generation: u64,
    /// Commit head.
    head: Location,
    /// Offset to write new item at.
    free_offset: i64,
    /// Used to ensure root is valid. Write could be interrupted
    /// or corrupted.
    checksum: u64,
}

impl Root {
    fn new() -> Self {
        Self {
            generation: 0,
            head: Location::new(usize::MAX, usize::MAX),
            free_offset: FREE_START,
            checksum: 0,
        }
    }

    fn calc_checksum(&self) -> u64 {
        let mut hasher = aranya_crypto::siphasher::sip::SipHasher::new();
        hasher.write_u64(self.generation);
        hasher.write_usize(self.head.segment);
        hasher.write_usize(self.head.command);
        hasher.write_i64(self.free_offset);
        hasher.finish()
    }

    fn validate(self) -> Result<Self, StorageError> {
        if self.checksum != self.calc_checksum() {
            // TODO(jdygert): Isn't really a bug.
            println!("invalid checksum");
        }
        Ok(self)
    }
}

/// A file-based reader for linear storage.
#[derive(Clone)]
pub struct Reader {
    file: FileHandle,
}

impl Read for Reader {
    fn fetch<T>(&self, offset: usize) -> Result<T, StorageError>
    where
        T: DeserializeOwned,
    {
        let off = i64::try_from(offset).map_err(|_| StorageError::IoError)?;
        self.file.load(off)
    }
}

#[derive(Clone)]
struct FileHandle {
    // note RawFile is clonable, but we prefer this as we want a collection of pointers to one RawFile, not multiple
    volume_manager: Arc<VolumeMan>,
    fd: Arc<RawFile>,
}

impl FileHandle {
    fn read_exact(&self, mut offset: i64, mut buf: &mut [u8]) -> Result<(), StorageError> {
        while !buf.is_empty() {
            match self.volume_manager.read(*self.fd.as_ref(), buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = buf.get_mut(n..).unwrap();
                    offset = offset.checked_add(i64::try_from(n).unwrap()).unwrap()
                }
                Err(e) => {
                    panic!("{:?}", e);
                }
            }
        }
        if !buf.is_empty() {
            println!(
                "could not fill buffer. remaining: {remaining}",
                remaining = buf.len()
            );
            Err(StorageError::IoError)
        } else {
            Ok(())
        }
    }

    fn write_all(&self, mut offset: i64, buf: &[u8]) -> Result<(), StorageError> {
        while !buf.is_empty() {
            match self.volume_manager.write(*self.fd.clone(), buf) {
                Ok(()) => {
                    // ! This is where things can start going awry as it writes all, not parts so if greater than buffer it crashes
                    let n = buf.len();
                    offset = offset.checked_add(i64::try_from(n).unwrap()).unwrap();
                }
                Err(e) => {
                    panic!("{:?}", e);
                }
            }
        }
        Ok(())
    }

    fn sync(&self) -> Result<(), StorageError> {
        self.volume_manager.flush_file(*self.fd.clone());
        Ok(())
    }

    fn dump<T: Serialize>(&self, offset: i64, value: &T) -> Result<i64, StorageError> {
        let bytes = postcard::to_allocvec(value).map_err(|err| {
            println!("dump {:?}", err);
            StorageError::IoError
        })?;
        let len: u32 = bytes.len().try_into().unwrap();
        self.write_all(offset, &len.to_be_bytes())?;
        let offset2 = offset.checked_add(4).unwrap();
        self.write_all(offset2, &bytes)?;
        let off = offset2.checked_add(len.into()).unwrap();
        Ok(off)
    }

    fn load<T: DeserializeOwned>(&self, offset: i64) -> Result<T, StorageError> {
        let mut bytes = [0u8; 4];
        self.read_exact(offset, &mut bytes)?;
        let len = u32::from_be_bytes(bytes);
        let mut bytes = alloc::vec![0u8; len as usize];
        self.read_exact(offset.checked_add(4).unwrap(), &mut bytes)?;
        postcard::from_bytes(&bytes).map_err(|err| {
            println!("load {:?}", err);
            StorageError::IoError
        })
    }
}

// Max character size limit is 8 before period and 3 after "https://github.com/rust-embedded-community/embedded-sdmmc-rs/blob/011726bf85edfd1315732746ccb1d8c5c7fec5b4/src/filesystem/filename.rs#L12"
fn filename_8_3(id: GraphId) -> String {
    let name = id.to_base58();

    let truncated_name = if name.len() > 8 {
        name[..8].to_string()
    } else {
        name.to_string()
    };

    format!("{}.bin", truncated_name)
}
