use std::{fs::OpenOptions, path::PathBuf};

use clap::Parser;
use parameter_store::*;

#[derive(Parser, Debug)]
struct Args {
    file: PathBuf,
    /// Set the IR interface address
    #[arg(long)]
    ir_address: Option<u16>,
    /// Set the IR peer addresses
    #[arg(long)]
    ir_peers: Option<String>,
    /// Set the device's color r,g,b
    #[arg(long)]
    color: Option<String>,
    #[arg(short, long)]
    create: bool,
    #[arg(short, long)]
    verbose: bool,
}

pub fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let f = OpenOptions::new()
        .create_new(args.create)
        .read(true)
        .write(true)
        .open(args.file)?;
    let io = FileIO::new(f);
    let mut store: ParameterStore<Parameters, _> = ParameterStore::new(io);
    let mut params = if args.create {
        let p = Parameters::default();
        store.store(&p)?;
        p
    } else {
        store.fetch()?
    };
    let mut modified = false;

    if let Some(address) = args.ir_address {
        params.address = address;
        modified = true;
    }
    if let Some(peers) = args.ir_peers {
        params.peers = peers
            .split(',')
            .map(|c| c.parse::<u16>().expect("invalid IR address"))
            .collect();
        modified = true;
    }
    if let Some(color) = args.color {
        let components: Vec<u8> = color
            .split(',')
            .map(|c| c.parse::<u8>().expect("invalid color component"))
            .collect();
        if components.len() != 3 {
            panic!("Must specify three color components: r, g, b");
        }
        params.color = RgbU8 {
            red: components[0],
            green: components[1],
            blue: components[2],
        }
    }

    if modified {
        store.store(&params)?;
    }

    if !modified || args.verbose {
        println!(
            "Graph ID: {}",
            params
                .graph_id
                .map(|v| format!("{v}"))
                .unwrap_or(String::from("None"))
        );
        println!("IR address: {}", params.address);
        println!("IR peer addresses: {:?}", params.peers);
        println!("Color: {:?}", params.color);
    }
    Ok(())
}
