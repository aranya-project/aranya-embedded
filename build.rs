use aranya_crypto;
use aranya_crypto_ffi;
use aranya_device_ffi;
use aranya_envelope_ffi;
use aranya_policy_compiler::Compiler;
use aranya_policy_lang::lang::parse_policy_document;
use aranya_policy_vm::ffi::{FfiModule, ModuleSchema};
use aranya_policy_vm::Module;
use ciborium::de::from_reader;
use ciborium::ser::into_writer;
use ron::de::from_str;
use serde::Deserialize;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

#[derive(serde::Deserialize)]
pub struct AccessPointConfiguration {
    pub ssid: String,
    pub ssid_hidden: bool,
    pub channel: u8,
    pub secondary_channel: Option<u8>,
    pub protocols: Vec<Protocol>,
    pub auth_method: AuthMethod,
    pub password: String,
    pub max_connections: u16,
}

#[derive(Debug, Default, Deserialize)]
pub enum Protocol {
    P802D11B,
    P802D11BG,
    #[default]
    P802D11BGN,
    P802D11BGNLR,
    P802D11LR,
    P802D11BGNAX,
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
    let config: AccessPointConfiguration = from_str(&config_str)?;

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
use esp_wifi::wifi::{{AccessPointConfiguration, AuthMethod, Protocol}};
use heapless::String;

pub fn wifi_config() -> AccessPointConfiguration {{
    AccessPointConfiguration {{
        ssid: String::<32>::from_str("{}").expect("SSID Error"),
        ssid_hidden: {},
        channel: {},
        secondary_channel: {:?},
        protocols: ({}).into(),
        auth_method: AuthMethod::{:?},
        password: String::<64>::from_str("{}").expect("Password Error"),
        max_connections: {},
    }}
}}
"#,
        config.ssid,
        config.ssid_hidden,
        config.channel,
        config.secondary_channel,
        protocols_to_bitflag(&config.protocols),
        config.auth_method,
        config.password,
        config.max_connections
    );

    File::create(&dest_path)?.write_all(content.as_bytes())?;

    aranya_setup();

    // Tell Cargo to rerun this if files change
    println!("cargo:rerun-if-changed=config/wifi.ron");
    println!("cargo:rerun-if-changed=config/policy.md");
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}

fn protocols_to_bitflag(protocols: &[Protocol]) -> String {
    if protocols.is_empty() {
        return "Protocol::P802D11BGN".to_string();
    }

    protocols
        .iter()
        .map(|p| format!("Protocol::{:?}", p))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn aranya_setup() {
    let ffi_schema: &[ModuleSchema<'static>] = &[
        aranya_envelope_ffi::Ffi::SCHEMA,
        aranya_crypto_ffi::Ffi::<aranya_crypto::keystore::memstore::MemStore>::SCHEMA,
        aranya_device_ffi::FfiDevice::SCHEMA,
        aranya_idam_ffi::Ffi::<aranya_crypto::keystore::memstore::MemStore>::SCHEMA,
        aranya_perspective_ffi::FfiPerspective::SCHEMA,
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
    let mut serialized = Vec::new();
    into_writer(&module, &mut serialized).expect("Failed to serialize Module");

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
        from_reader(&serialized[..]).expect("Failed to deserialize Module in build script");
}
