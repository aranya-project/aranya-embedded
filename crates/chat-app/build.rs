use std::{
    env,
    fs::{self, File},
    io::Write,
    path::Path,
};

use aranya_policy_compiler::Compiler;
use aranya_policy_lang::lang::parse_policy_document;
use aranya_policy_vm::{
    ffi::{FfiModule, ModuleSchema},
    Module,
};
use envelope_ffi::NullEnvelope;
use rkyv::rancor::Error;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    aranya_setup();

    // Tell Cargo to rerun this if files change
    println!("cargo:rerun-if-changed=config/policy.md");
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}

fn aranya_setup() {
    let ffi_schema: &[ModuleSchema<'static>] = &[NullEnvelope::SCHEMA];
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
