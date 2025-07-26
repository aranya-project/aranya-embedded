use aranya_policy_compiler::Compiler;
use aranya_policy_lang::lang::parse_policy_document;
use aranya_policy_vm::ffi::{FfiModule, ModuleSchema};
use aranya_policy_vm::Module;
use rkyv::rancor::Error;
use ron::de::from_str;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

#[derive(serde::Deserialize)]
pub struct BaseConfiguration {
    /// The SSID of the Wi-Fi network.
    pub ssid: String,
    /// The password for the Wi-Fi connection.
    pub password: String,
}

fn wifi_config() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = Path::new("config/wifi.ron");

    // Check if config exists
    if !config_path.exists() {
        panic!("wifi.ron configuration file not found! Please copy wifi.ron.template to wifi.ron and fill it out.");
    }

    // Read and validate the configuration
    let config_str = fs::read_to_string(config_path)?;
    let config: BaseConfiguration = from_str(&config_str)?;

    // Setup output path
    let out_dir = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("src/built/wifi_config.rs");

    // Create parent directories if they don't exist
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write the configuration to file
    let content = format!(
        r#"pub const WIFI_SSID: &str = "{ssid}";
pub const WIFI_PASSWORD: &str = "{pass}";
"#,
        ssid = config.ssid,
        pass = config.password,
    );

    File::create(&dest_path)?.write_all(content.as_bytes())?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    //wifi_config();
    aranya_setup();

    // Tell Cargo to rerun this if files change
    println!("cargo:rerun-if-changed=config/wifi.ron");
    println!("cargo:rerun-if-changed=config/policy.md");
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}

include!("src/aranya/envelope.rs");

fn aranya_setup() {
    let ffi_schema: &[ModuleSchema<'static>] =
        &[NullEnvelope::SCHEMA];
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

    // Generate interface
    println!("cargo:rerun-if-changed=config/policy.md");
    aranya_policy_ifgen_build::generate("config/policy.md", "src/aranya/policy.rs").unwrap();
}
