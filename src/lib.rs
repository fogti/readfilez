extern crate boolinator;
extern crate memmap;

use boolinator::Boolinator;
use memmap::{Mmap, MmapOptions};
use std::{fs, io, io::Read, isize};

// private interface

fn open_as_mmap(fh: &fs::File, len: usize) -> io::Result<Mmap> {
    Ok(unsafe {
        // NOTE: replace map_copy?.make_read_only? with map_copy_read_only?
        // once issue danburkert/memmap-rs#81 is fixed
        MmapOptions::new()
            .len(len)
            .map_copy(&fh)?
            .make_read_only()?
    })
}

// public interface

/// Returns the length of the file,
/// and is baed upon [`memmap::MmapOptions::get_len()`].
/// It sanitises the fact that mapping a slice greater than isize::MAX
/// has undefined behavoir.
pub fn get_file_len(fh: &fs::File) -> Option<usize> {
    fh.metadata()
        .ok()
        .map(|x| x.len())
        .and_then(|len| (len != 0 && len <= (isize::MAX as u64)).as_some(len as usize))
}

/// buffered or mmapped file contents handle
pub enum FileHandle {
    Mapped(Mmap),
    Buffered(Vec<u8>),
}

use self::FileHandle::*;

impl FileHandle {
    /// This function returns a slice pointing to
    /// the contents of the [`FileHandle`].
    #[inline]
    pub fn get_slice(&self) -> &[u8] {
        match self {
            Mapped(ref dt) => &dt[..],
            Buffered(ref dt) => &dt[..],
        }
    }
}

/// Reads the file contents
pub fn read_from_file(fh: io::Result<fs::File>) -> io::Result<FileHandle> {
    let fh = fh?;
    let len = get_file_len(&fh);

    // do NOT try to map the file if the size is unknown
    if let Some(ret) = len
        .and_then(|len| open_as_mmap(&fh, len).ok())
        .map(Mapped)
    {
        return Ok(ret);
    }
    let mut contents = Vec::with_capacity(len.unwrap_or(0) + 1);
    io::BufReader::new(fh).read_to_end(&mut contents)?;
    contents.shrink_to_fit();
    Ok(Buffered(contents))
}
