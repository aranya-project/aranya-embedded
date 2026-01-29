mod internal;
mod linear;
mod plathacks;

use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use aranya_runtime::VmProtocolData;
use clap::Parser;
use rkyv::rancor;

use crate::{internal::*, linear::*};

#[derive(clap::Parser)]
struct Args {
    /// the input binary partition dump
    file: PathBuf,
    /// The output DOT file
    out: PathBuf,
}

struct Dump {
    buf: Vec<u8>,
}

impl Dump {
    pub fn new(p: &Path) -> Dump {
        let mut f = OpenOptions::new()
            .read(true)
            .open(p)
            .expect("could not open file");

        let mut buf = vec![];
        f.read_to_end(&mut buf).expect("could not read file");
        Dump { buf }
    }

    fn get_head(&self) -> EspStorageHeader {
        assert_eq!(&self.buf[0..4], HEADER_MAGIC, "bad header magic");
        let header =
            rkyv::access::<ArchivedEspStorageHeader, rancor::Error>(&self.buf[4..4 + HEADER_SIZE])
                .expect("could not deserialize header");
        rkyv::deserialize::<EspStorageHeader, rancor::Error>(header)
            .expect("could not deserialize header")
    }

    fn get_segment(&self, offset: u32) -> SegmentRepr {
        println!("{:04X}", DATA_OFFSET + (offset as usize));
        let seg_buf = &self.buf[DATA_OFFSET + (offset as usize)..];
        assert_eq!(
            &seg_buf[0..4],
            SEGMENT_HEADER_MAGIC,
            "bad segment header magic"
        );
        let mut rkyv_buf = rkyv::util::Align([0u8; SEGMENT_HEADER_SIZE]); // #*&@ rkyv alignment...
        rkyv_buf.copy_from_slice(&seg_buf[4..4 + SEGMENT_HEADER_SIZE]);
        let header = rkyv::access::<ArchivedSegmentHeader, rancor::Error>(rkyv_buf.as_slice())
            .expect("could not deserialize header");
        rkyv::deserialize::<SegmentHeader, rancor::Error>(header)
            .expect("could not deserialize header");

        let seg_buf = &seg_buf[4 + SEGMENT_HEADER_SIZE..];
        postcard::from_bytes(seg_buf).expect("could not deserialize segment")
    }

    fn get_graph(&self, offset: u32) -> BTreeMap<u32, SegmentRepr> {
        let mut map = BTreeMap::new();
        let seg = self.get_segment(offset);
        let prior = seg.prior;
        map.insert(offset, seg);
        match prior {
            aranya_runtime::Prior::None => (),
            aranya_runtime::Prior::Single(l) => {
                let mut children = self.get_graph(l.segment as u32);
                map.append(&mut children);
            }
            aranya_runtime::Prior::Merge(l1, l2) => {
                let mut children = self.get_graph(l1.segment as u32);
                map.append(&mut children);
                let mut children = self.get_graph(l2.segment as u32);
                map.append(&mut children);
            }
        }
        map
    }
}

fn make_dot(graph: BTreeMap<u32, SegmentRepr>, p: &Path) -> anyhow::Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(p)?;

    writeln!(f, "digraph G {{")?;
    writeln!(f, "    rankdir=LR")?;
    for (offset, segment) in graph {
        // let mut links = vec![];
        writeln!(f, "    subgraph cluster_segment_{offset} {{")?;
        writeln!(f, "        label=\"Segment {offset}\";")?;
        writeln!(f, "        shape=rectangle;")?;
        for command_data in segment.commands {
            let mut short_id = command_data.id.to_string();
            short_id.truncate(8);
            write!(
                f,
                "        command_{} [label=\"{}",
                command_data.id, short_id
            )?;
            let _command: VmProtocolData = postcard::from_bytes(&command_data.data)?;
            // TODO: Update to latest format.
            // Tricky since parent ID of first command is only known by looking at previous segments.
            // Recommend switching command names to use location instead of ID.

            // match command {
            //     VmProtocolData::Init { kind, .. } => {
            //         writeln!(f, "\\n({kind})\", style=filled, color=lightblue];")?
            //     }
            //     VmProtocolData::Basic { parent, kind, .. } => {
            //         writeln!(f, "\\n({kind})\"];")?;
            //         links.push((command_data.id, parent.id));
            //     }
            //     VmProtocolData::Merge { left, right } => {
            //         writeln!(f, "\", style=filled, color=lightgreen];")?;
            //         links.push((command_data.id, left.id));
            //         links.push((command_data.id, right.id));
            //     }
            // }
        }
        writeln!(f, "    }}")?;
        // for (from, to) in links {
        //     writeln!(f, "    command_{from} -> command_{to}")?;
        // }
    }
    writeln!(f, "}}")?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let dump = Dump::new(&args.file);

    let head = dump.get_head();
    println!("{head:?}");
    let graph = dump.get_graph(head.head.unwrap().0);

    make_dot(graph, &args.out)?;

    Ok(())
}
