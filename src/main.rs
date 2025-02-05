use std::fs::File;
use std::io::prelude::*;

use esp_idf_svc::fs::littlefs::Littlefs;
use esp_idf_svc::io::vfs::MountedLittlefs;

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Hello, world!");

    log::info!("Grabbing the LittleFS partition");
    let mut littlefs = unsafe { Littlefs::<()>::new_partition("storage")? };

    log::info!("Formatting LittleFS partition");
    littlefs.format()?;

    log::info!("Mounting the LittleFS partition");
    let mount = MountedLittlefs::mount(littlefs, "/littlefs")?;

    {
        let mut file = File::create("/littlefs/hello_world.txt")?;
        file.write_all(b"This is data read from a file!")?;
    }

    let data = std::fs::read("/littlefs/hello_world.txt")?;
    log::info!("{}", std::str::from_utf8(&data)?);

    let info = mount.info()?;
    log::info!(
        "Total: {} bytes, Used: {} bytes",
        info.total_bytes,
        info.used_bytes
    );

    Ok(())
}
