use alloc::rc::Rc;
use alloc::string::{String, ToString};

use alloc::vec::Vec;
use alloc::{format, vec};
use aranya_runtime::{linear, GraphId, Location, StorageError};
use embedded_sdmmc::{Mode, RawFile, VolumeIdx};
use esp_println::println;
use owo_colors::OwoColorize;
use postcard::{from_bytes, to_allocvec};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::VolumeMan;

/*
Details of Implementation
Location details as I understand it can be arbitrary so long as the pattern is consistent and applicably information for storage. The pattern were using:
Location is defined as:
    segment: offset in bytes for SD card file
    command: offset in command number for SD card file
}

The data binary is all command data
The head binary is the head location
The location binary represents a vector which iterates through every command in data with the corresponding location for the bytes offset of each command, so that we know the boundaries for deserialization
*/

// Single threaded implementation therefore we only need Rc and RefCell, not Rc and Mutex as race conditions are not a concern. We are using Rc<RefCell<_>> as managing memory by passing the peripherals is difficult while fulfilling trait implementations and we want shared ownership of one peripheral. (It's likely that one can have a cleaner implementation but this is good enough)
// ! todo remove redundant Rc<>
// ! Todo, remove truncate so it doesn't reset
pub struct GraphManager {
    vol: Rc<VolumeMan>,
}

impl GraphManager {
    /// Creates a `FileManager` at `dir`.
    pub fn new(volume_mgr: Rc<VolumeMan>) -> Result<Self, StorageError> {
        Ok(Self { vol: volume_mgr })
    }
}

pub struct GraphWriter {
    vol: Rc<VolumeMan>,
    /// Deserialize locations are used as usize markers for where deserialize start and end points should be on the graph data file
    location_file: Rc<RawFile>,
    /// Raw binary data of the graph
    data_file: Rc<RawFile>,
    /// Current head location
    head_file: RawFile,
}

impl GraphWriter {
    /// Creates a `FileManager` at `dir`.
    pub fn new(
        volume_mgr: Rc<VolumeMan>,
        location_file: Rc<RawFile>,
        data_file: Rc<RawFile>,
        head_file: RawFile,
    ) -> Self {
        Self {
            vol: volume_mgr,
            location_file,
            data_file,
            head_file,
        }
    }
}

impl linear::io::Write for GraphWriter {
    type ReadOnly = GraphReader;

    fn readonly(&self) -> Self::ReadOnly {
        GraphReader {
            vol: self.vol.clone(),
            deserialize_locations_file_handle: self.location_file.clone(),
            graph_data_file_handle: self.data_file.clone(),
        }
    }

    fn head(&self) -> Result<Location, StorageError> {
        println!("{}", "Head Access".green());

        let mut serialize_vec: Vec<u8> = Vec::new();
        while !self
            .vol
            .file_eof(self.head_file)
            .map_err(|_| StorageError::IoError)?
        {
            let mut buffer: [u8; 16] = [0; 16];
            let characters_parsed = self.vol.read(self.head_file, &mut buffer).map_err(|e| {
                println!(
                    "Failed to Read Characters Into a Buffer: {:?}",
                    format!("{:?}", e).red()
                );
                StorageError::IoError
            })?;
            serialize_vec.extend_from_slice(&buffer[..characters_parsed]);
        }

        println!(
            "{}",
            format!("Read Head Binaries: {:?}", serialize_vec).blue()
        );

        let head_location: Location = if serialize_vec.is_empty() {
            Location::new(0, 0)
        } else {
            from_bytes(&serialize_vec).map_err(|e| {
                println!(
                    "Something has Gone Wrong in Serialization or Deserialization: {}",
                    format!("{:?}", e).red()
                );
                StorageError::IoError
            })?
        };
        println!("{}", format!("Head Location: {:?}", head_location).blue());
        Ok(head_location)
    }

    fn append<F, T>(&mut self, builder: F) -> Result<T, StorageError>
    where
        F: FnOnce(usize) -> T,
        T: Serialize,
    {
        println!("{}", "Append Data".green());
        let data_file_length = self
            .vol
            .file_length(*self.data_file)
            .map_err(|_| StorageError::IoError)?;
        println!(
            "{}",
            format!("Bytes of Data Binary: {}", data_file_length).blue()
        );
        let data = builder(data_file_length as usize);
        println!(
            "{}",
            "Built Data from Builder using End of Data File".green()
        );
        let serialized_data = to_allocvec(&data).map_err(|e| {
            println!("Failed to Serialize Module: {:?}", format!("{:?}", e).red());
            StorageError::IoError
        })?;
        if serialized_data.is_empty() {
            panic!("Serialized Data to Append Cannot be Empty")
        }
        self.vol
            .write(*self.data_file, &serialized_data)
            .map_err(|_| StorageError::IoError)?;
        println!(
            "{}",
            format!("Serialized Graph Data Written: {:?}", serialized_data).blue()
        );

        println!("{}", "Location Management".green());

        let mut serialized_location = Vec::new();
        while !self
            .vol
            .file_eof(*self.location_file)
            .map_err(|_| StorageError::IoError)?
        {
            let mut buffer: [u8; 16] = [0; 16];
            let characters_parsed = self
                .vol
                .read(*self.location_file, &mut buffer)
                .map_err(|_| StorageError::IoError)?;
            serialized_location.extend_from_slice(&buffer[..characters_parsed]);
        }

        println!(
            "{}",
            format!("Reading Locations Binary: {:?}", serialized_location).blue()
        );

        let mut locations_vec: Vec<usize> = if serialized_location.is_empty() {
            Vec::new()
        } else {
            from_bytes(&serialized_location).map_err(|e| {
                println!(
                    "Something has Gone Wrong in Serialization or Deserialization: {}",
                    format!("{:?}", e).red()
                );
                StorageError::IoError
            })?
        };
        println!(
            "{}",
            format!("Appending to Locations Binary: {:?}", locations_vec).blue()
        );
        let append_bytes_location =
            locations_vec.last().map_or(0, |&last| last) + serialized_data.len();
        locations_vec.push(append_bytes_location);

        let serialized = to_allocvec(&locations_vec).map_err(|e| {
            println!(
                "Failed to Serialize Location Binary: {:?}",
                format!("{:?}", e).red()
            );
            StorageError::IoError
        })?;
        self.vol
            .file_seek_from_start(*self.location_file, 0)
            .map_err(|e| {
                println!(
                    "Failed to set Cursor to Offset {} Bytes From the Start of File: {:?}",
                    0,
                    format!("{:?}", e).red()
                );
                StorageError::IoError
            })?;
        self.vol
            .write(*self.location_file, &serialized)
            .map_err(|e| {
                println!(
                    "Failed to set Write to File: {:?}",
                    format!("{:?}", format!("{:?}", e).red())
                );
                StorageError::IoError
            })?;
        println!(
            "{}",
            format!(
                "Location File Written.\n\tDebug is: {:?}\n\tBytes are: {:?}",
                locations_vec, serialized
            )
            .blue()
        );
        Ok(data)
    }

    fn commit(&mut self, head: Location) -> Result<(), StorageError> {
        println!("{}", "Commit".green());
        println!("{}", format!("Commit Head Details: {}", head).blue());
        println!("{}", "Head Management".green());
        let mut serialized = Vec::new();
        while !self
            .vol
            .file_eof(self.head_file)
            .map_err(|_| StorageError::IoError)?
        {
            let mut buffer: [u8; 16] = [0; 16];
            let characters_parsed = self
                .vol
                .read(self.head_file, &mut buffer)
                .map_err(|_| StorageError::IoError)?;
            serialized.extend_from_slice(&buffer[..characters_parsed]);
        }

        println!("{}", format!("Read Head Binaries: {:?}", serialized).blue());

        let location_head: Location = if serialized.is_empty() {
            Location::new(0, 0)
        } else {
            from_bytes(&serialized).map_err(|e| {
                println!(
                    "Something has Gone Wrong in Serialization or Deserialization: {}",
                    format!("{:?}", e).red()
                );
                StorageError::IoError
            })?
        };

        println!("{}", format!("Current Head: {}", location_head).blue());

        let serialized = to_allocvec(&head).map_err(|e| {
            println!("Failed to Serialize Module: {:?}", format!("{:?}", e).red());
            StorageError::IoError
        })?;
        self.vol
            .file_seek_from_start(self.head_file, 0)
            .map_err(|e| {
                println!(
                    "Failed to set cursor to offset {} bytes from the start of file: {:?}",
                    0,
                    format!("{:?}", e).red()
                );
                StorageError::IoError
            })?;
        self.vol.write(self.head_file, &serialized).map_err(|e| {
            println!(
                "Failed to set write to file: {:?}",
                format!("{:?}", format!("{:?}", e).red())
            );
            StorageError::IoError
        })?;
        println!(
            "{}",
            format!(
                "Head file written.\n\tDebug is: {:?}\n\tBytes are: {:?}",
                location_head, serialized
            )
            .blue()
        );
        // For head location one might be worried that a data structure being naively written in a manner that doesn't append data exclusively similar to location or data management could allow tail information that causes errors for deserialization. This isn't a problem as the values in the Head location exclusively increase meaning that it will similarly continue to have a serialization size of either the same or greater of its previous iteration and overwrite all data, preventing deserialization errors due to lagging data
        // Append does this in one step. Separation between append and commit will be implemented later if necessary
        Ok(())
    }
}

#[derive(Clone)]
pub struct GraphReader {
    vol: Rc<VolumeMan>,
    deserialize_locations_file_handle: Rc<RawFile>,
    graph_data_file_handle: Rc<RawFile>,
}

impl linear::io::Read for GraphReader {
    fn fetch<T>(&self, offset: usize) -> Result<T, StorageError>
    where
        T: DeserializeOwned, // ! + Debug,
    {
        println!("{}", format!("Fetch offset {}", offset).green());

        let mut serialize_vec: Vec<u8> = Vec::new();
        while !self
            .vol
            .file_eof(*self.deserialize_locations_file_handle)
            .map_err(|_| StorageError::IoError)?
        {
            let mut buffer: [u8; 16] = [0; 16];
            let characters_parsed = self
                .vol
                .read(*self.deserialize_locations_file_handle, &mut buffer)
                .map_err(|_| StorageError::IoError)?;
            serialize_vec.extend_from_slice(&buffer[..characters_parsed]);
        }

        println!(
            "{}",
            format!("Read location binaries: {:?}", serialize_vec).blue()
        );

        let mut locations_vec: Vec<usize> = if serialize_vec.is_empty() {
            Vec::new()
        } else {
            from_bytes(&serialize_vec).map_err(|e| {
                println!(
                    "Something has gone wrong in serialization or deserialization: {}",
                    format!("{:?}", e).red()
                );
                StorageError::IoError
            })?
        };

        println!(
            "{}",
            format!("Match binary search locations: {:?}", locations_vec).blue()
        );
        let index = if offset != 0 {
            match locations_vec.binary_search(&offset) {
                Ok(index) => index,
                Err(e) => {
                    println!("Error Binary Search: {:?}", format!("{:?}", e).red());
                    println!("Searched for: {}\nLocations: {:?}", offset, locations_vec);
                    return Err(StorageError::PerspectiveHeadMismatch);
                }
            }
        } else {
            locations_vec.insert(0, 0);
            0
        };
        let offset_data_end = locations_vec
            .get(index + 1)
            .unwrap_or_else(|| panic!("Can't find subsequent location in vector"));
        println!(
            "{}",
            format!(
                "Post Binary Search.\n\tIndex is: {}\n\tStarting point of Data is: {}\n\tEnding point of Data is: {}",
                index, offset, offset_data_end
            )
            .blue()
        );

        println!("{}", "Start Database Fetch".green());
        let mut serialize_vec: Vec<u8> = vec![0; offset_data_end - offset];

        let characters_parsed = self
            .vol
            .read(*self.graph_data_file_handle, &mut serialize_vec)
            .map_err(|_| StorageError::IoError)?;

        if characters_parsed != offset_data_end - offset {
            panic!("Characters Parsed do not Match Index Distance.\n Data Start: {}, Data End: {}, Delta: {}, Characters Parsed: {}", offset, offset_data_end, offset_data_end - offset, characters_parsed);
        }

        println!(
            "{}",
            format!(
                "Serialized Values of Data After Location\n\tData: {:?}\n\tLength: {}",
                serialize_vec,
                serialize_vec.len()
            )
            .blue()
        );
        let deserialized_u: T = from_bytes(&serialize_vec[..]).map_err(|e| {
            println!(
                "Failed to Deserialize Module: {:?}",
                format!("{:?}", e).red()
            );
            StorageError::IoError
        })?;
        println!("{}", "Finish Fetch".green());
        Ok(deserialized_u)
    }
}

impl linear::io::IoManager for GraphManager {
    type Writer = GraphWriter;

    fn create(&mut self, id: GraphId) -> Result<Self::Writer, StorageError> {
        println!("{}", format!("Create GraphId: {:?}", id).green());
        println!("{}", "Create Files".green());
        // Check if file already exists by opening in ReadWriteCreate. Throw error if one does exist and create a file if it doesn't
        // Open all files using the same directory reference
        let raw_vol = self
            .vol
            .open_raw_volume(VolumeIdx(0))
            .map_err(|_| StorageError::IoError)?;
        let raw_dir = self
            .vol
            .open_root_dir(raw_vol)
            .map_err(|_| StorageError::IoError)?;

        let file_name = truncate_filename(format!("d_{}.b", id), 8);
        let data_file: Rc<RawFile> = Rc::new(
            self.vol
                .open_file_in_dir(raw_dir, file_name.as_str(), Mode::ReadWriteCreateOrTruncate)
                .map_err(|e| {
                    println!(
                        "Error Opening File {} in Root Directory: {:?}",
                        file_name,
                        format!("{:?}", e).red()
                    );
                    StorageError::NoSuchStorage
                })?,
        );

        let file_name = truncate_filename(format!("l_{}.b", id), 8);
        let location_file: Rc<RawFile> = Rc::new(
            self.vol
                .open_file_in_dir(raw_dir, file_name.as_str(), Mode::ReadWriteCreateOrTruncate)
                .map_err(|e| {
                    println!(
                        "Error Opening File {} in Root Directory: {:?}",
                        file_name,
                        format!("{:?}", e).red()
                    );
                    StorageError::NoSuchStorage
                })?,
        );

        let file_name = truncate_filename(format!("h_{}.b", id), 8);
        let head_file: RawFile = self
            .vol
            .open_file_in_dir(raw_dir, file_name.as_str(), Mode::ReadWriteCreateOrTruncate)
            .map_err(|e| {
                println!(
                    "Error Opening File {} in Root Directory: {:?}",
                    file_name,
                    format!("{:?}", e).red()
                );
                StorageError::NoSuchStorage
            })?;

        println!("{}", "Files Created".green());
        Ok(GraphWriter::new(
            self.vol.clone(),
            location_file,
            data_file,
            head_file,
        ))
    }

    // ! todo make this return Ok(None) if none exist instead of creating one and returning that
    fn open(&mut self, id: GraphId) -> Result<Option<Self::Writer>, aranya_runtime::StorageError> {
        println!("{}", format!("Open GraphId: {:?}", id).green());
        println!("{}", "Open Files".green());
        // Check if file exists by opening in ReadOnly mode. Return error if it doesn't
        let raw_vol = self
            .vol
            .open_raw_volume(VolumeIdx(0))
            .map_err(|_| StorageError::IoError)?;
        let raw_dir = self
            .vol
            .open_root_dir(raw_vol)
            .map_err(|_| StorageError::IoError)?;

        let file_name = truncate_filename(format!("d_{}.b", id), 8);
        let data_file: Rc<RawFile> = Rc::new(
            self.vol
                .open_file_in_dir(raw_dir, file_name.as_str(), Mode::ReadWriteTruncate)
                .map_err(|e| {
                    println!(
                        "Error Opening File {} in Root Directory: {:?}",
                        file_name,
                        format!("{:?}", e).red()
                    );
                    StorageError::NoSuchStorage
                })?,
        );

        let file_name = truncate_filename(format!("l_{}.b", id), 8);
        let location_file: Rc<RawFile> = Rc::new(
            self.vol
                .open_file_in_dir(raw_dir, file_name.as_str(), Mode::ReadWriteTruncate)
                .map_err(|e| {
                    println!(
                        "Error Opening File {} in Root Directory: {:?}",
                        file_name,
                        format!("{:?}", e).red()
                    );
                    StorageError::NoSuchStorage
                })?,
        );

        let file_name = truncate_filename(format!("h_{}.b", id), 8);
        let head_file: RawFile = self
            .vol
            .open_file_in_dir(raw_dir, file_name.as_str(), Mode::ReadWriteTruncate)
            .map_err(|e| {
                println!(
                    "Error Opening File {} in Root Directory: {:?}",
                    file_name,
                    format!("{:?}", e).red()
                );
                StorageError::NoSuchStorage
            })?;

        println!("{}", "Files Created".green());
        Ok(Some(GraphWriter::new(
            self.vol.clone(),
            location_file,
            data_file,
            head_file,
        )))
    }
}

// Max character size limit is 8 before period and 3 after "https://github.com/rust-embedded-community/embedded-sdmmc-rs/blob/011726bf85edfd1315732746ccb1d8c5c7fec5b4/src/filesystem/filename.rs#L12"
fn truncate_filename(name: String, max_base_length: usize) -> String {
    let mut parts = name.rsplitn(2, '.');
    let extension = parts.next().unwrap_or("").to_string();
    let base_name = parts.next().unwrap_or(&name).to_string();

    let truncated_base = if base_name.len() > max_base_length {
        base_name[..max_base_length].to_string()
    } else {
        base_name
    };

    if extension.is_empty() {
        truncated_base
    } else {
        format!("{}.{}", truncated_base, extension)
    }
}

// ! Implement RawFile closing within VolumeManager Arc or Rc upon error in sync/droping of this struct as it would result in blocking RawFile handles
