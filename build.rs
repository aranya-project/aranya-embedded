use aranya_policy_compiler::Compiler;
use aranya_policy_lang::lang::parse_policy_document;
use aranya_policy_vm::ffi::{FfiModule, ModuleSchema};
use aranya_policy_vm::Module;
use rkyv::rancor::Error;
use ron::de::from_str;
use serde::Deserialize;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

#[derive(serde::Deserialize)]
pub struct ClientConfiguration {
    /// The SSID of the Wi-Fi network.
    pub ssid: String,
    /// The BSSID (MAC address) of the client.
    pub bssid: Option<[u8; 6]>,
    // pub protocol: Protocol,
    /// The authentication method for the Wi-Fi connection.
    pub auth_method: AuthMethod,
    /// The password for the Wi-Fi connection.
    pub password: String,
    /// The Wi-Fi channel to connect to.
    pub channel: Option<u8>,
}

#[derive(Debug, Default, Deserialize)]
pub enum AuthMethod {
    None,
    WEP,
    WPA,
    #[default]
    WPA2Personal,
    WPAWPA2Personal,
    WPA2Enterprise,
    WPA3Personal,
    WPA2WPA3Personal,
    WAPIPersonal,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = Path::new("config/wifi.ron");

    // Check if config exists
    if !config_path.exists() {
        panic!("wifi.ron configuration file not found! Please copy wifi.ron.template to wifi.ron and fill it out.");
    }

    // Read and validate the configuration
    let config_str = fs::read_to_string(config_path)?;
    let config: ClientConfiguration = from_str(&config_str)?;

    // Setup output path
    let out_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("src/built/wifi_config.rs");

    // Create parent directories if they don't exist
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write the configuration to file
    let content = format!(
        r#"use core::str::FromStr;
use esp_wifi::wifi::{{AuthMethod, ClientConfiguration}};
use heapless::String;

pub fn wifi_config() -> ClientConfiguration {{
    ClientConfiguration {{
        ssid: String::from_str("{ssid}").unwrap(),
        bssid: {bssid},
        auth_method: AuthMethod::{auth:?},
        password: String::from_str("{pass}").unwrap(),
        channel: {channel:?},
    }}
}}
"#,
        ssid = config.ssid,
        bssid = match config.bssid {
            Some(bytes) => format!(
                "Some([0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}])",
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
            ),
            None => "None".to_string(),
        },
        auth = config.auth_method,
        pass = config.password,
        channel = config.channel,
    );

    File::create(&dest_path)?.write_all(content.as_bytes())?;

    aranya_setup();

    // Tell Cargo to rerun this if files change
    println!("cargo:rerun-if-changed=config/wifi.ron");
    println!("cargo:rerun-if-changed=config/policy.md");
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}

fn aranya_setup() {
    let ffi_schema: &[ModuleSchema<'static>] = &[
        aranya_envelope_ffi::Ffi::SCHEMA,
        aranya_crypto_ffi::Ffi::<aranya_crypto::keystore::memstore::MemStore>::SCHEMA,
        aranya_device_ffi::FfiDevice::SCHEMA,
        aranya_perspective_ffi::FfiPerspective::SCHEMA,
        aranya_idam_ffi::Ffi::<aranya_crypto::keystore::memstore::MemStore>::SCHEMA,
    ];
    // Parse policy
    let ast =
        parse_policy_document(include_str!("config/policy.md")).expect("parse policy document");

    // Compile AST
    let module = Compiler::new(&ast)
        .ffi_modules(ffi_schema)
        .compile()
        .expect("Failed to compile AST");

    // Serialize module
    // ! Find out why postcard fails
    let serialized = rkyv::to_bytes::<Error>(&module).expect("Failed to serialize Module");

    // Write to src/policy.rs
    let out_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("src/built/serialized_policy.bin");

    // Create all parent directories if they don't exist
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create directories");
    }

    let mut f = File::create(dest_path).expect("Failed to create file");

    f.write_all(&serialized)
        .expect("Failed to write serialized data");

    // Verify deserialization ability
    let _: Module =
        rkyv::from_bytes::<Module, Error>(&serialized).expect("Failed to serialize Module");
}
