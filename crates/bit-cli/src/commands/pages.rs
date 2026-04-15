use clap::Args;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const PAGE_SIZE: usize = 4096;

#[derive(Args)]
pub struct PagesArgs {
    /// Path to .bitstore file
    pub store: String,
}

pub fn run(args: &PagesArgs) -> Result<(), Box<dyn Error>> {
    let path = Path::new(&args.store);
    let mut file = File::open(path)?;

    // Read header to get page count
    let mut header_buf = [0u8; PAGE_SIZE];
    file.read_exact(&mut header_buf)?;

    let magic = &header_buf[0..4];
    if magic != b"BITS" {
        return Err("Not a valid .bitstore file (bad magic)".into());
    }
    let version = u32::from_le_bytes(header_buf[4..8].try_into().unwrap());
    if version != 2 {
        return Err(format!("Expected page-based store (version 2), got version {version}").into());
    }
    let page_count = u32::from_le_bytes(header_buf[12..16].try_into().unwrap());

    println!("Page 0:  HEADER");

    for page_num in 1..page_count {
        let offset = page_num as u64 * PAGE_SIZE as u64;
        file.seek(SeekFrom::Start(offset))?;
        let mut type_byte = [0u8; 1];
        file.read_exact(&mut type_byte)?;

        let label = match type_byte[0] {
            0x01 => "HEADER",
            0x02 => "BTREE_INTERIOR",
            0x03 => "BTREE_LEAF",
            0x04 => "OVERFLOW",
            0x05 => "FREELIST",
            other => {
                println!("Page {}:  UNKNOWN (0x{:02x})", page_num, other);
                continue;
            }
        };
        println!("Page {}:  {}", page_num, label);
    }

    Ok(())
}
