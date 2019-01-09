extern crate memmap;

use std::{fs, io, io::Read, usize};
use memmap::{Mmap,MmapOptions};

fn open_as_mmap(fh: &fs::File, len: usize) -> io::Result<Mmap> {
    Ok(unsafe {
        MmapOptions::new()
            .len(len)
            .map_copy(&fh)?
            .make_read_only()?
    })
}

/// returns the length of the file
// ORIGINAL SOURCE: memmap:MmapOptions.get_len
pub fn get_file_len(fh: &fs::File) -> Option<usize> {
    if let Ok(meta) = fh.metadata() {
        let len = meta.len();
        if len <= (usize::MAX as u64) {
            return Some(len as usize)
        }
    }
    None
}

pub enum FileHandle {
    Mapped(Mmap),
    Buffered(Vec<u8>),
}

use self::FileHandle::*;

impl FileHandle {
    pub fn get_slice(&self) -> &[u8] {
        match self {
            Mapped(  ref dt) => &dt[..],
            Buffered(ref dt) => &dt[..],
        }
    }
}

pub fn read_from_file(fh: io::Result<fs::File>) -> io::Result<FileHandle> {
    let fh = fh?;
    let len = get_file_len(&fh).unwrap_or(0);
    if let Ok(pf) = open_as_mmap(&fh, len) {
        return Ok(Mapped(pf));
    }
    let mut contents = Vec::with_capacity(len + 1);
    io::BufReader::new(fh).read_to_end(&mut contents)?;
    Ok(Buffered(contents))
}
