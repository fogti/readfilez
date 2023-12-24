use crate::{get_file_len, FileHandle, FileHandle::*, LengthSpec};
use std::{
    fs::File,
    io::{self, Read, Seek},
};

// private interface

fn open_as_mmap(fh: &File, offset: u64, len: usize) -> io::Result<memmap2::Mmap> {
    unsafe {
        memmap2::MmapOptions::new()
            .offset(offset)
            .len(len)
            .map_copy_read_only(fh)
    }
}

/// Reads a part of the file contents,
/// use this if the file is too big and needs to be read in parts,
/// starting at [`offset`] and until the given LengthSpec is met.
///
/// @param flen_hint : used to cache the call to [`get_file_len`]
pub(crate) fn read_part_from_file_intern(
    fh: &mut File,
    offset: u64,
    lenspec: LengthSpec,
    flen_hint: Option<u64>,
) -> io::Result<FileHandle> {
    // evaluate file length
    let evl: Option<usize> = {
        let maxlen_i = std::isize::MAX as usize;

        if lenspec.is_exact && lenspec.bound.map(|len| len > maxlen_i) == Some(true) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "length is too big",
            ));
        }

        [
            lenspec.bound,
            flen_hint
                .or_else(|| get_file_len(fh))
                .map(|lx| (lx - offset) as usize),
        ]
        .iter()
        .flatten()
        .min()
        .and_then(|&mxl| if mxl < maxlen_i { Some(mxl) } else { None })
    };

    // check common cases
    match evl {
        Some(0) => {
            return Ok(Buffered(Vec::new().into()));
        }
        Some(lx) => {
            // do NOT try to map the file if the size is unknown
            if let Ok(ret) = open_as_mmap(fh, offset, lx) {
                return Ok(Mapped(ret));
            }
        }
        None => {}
    }

    // use Buffered as fallback
    fh.seek(io::SeekFrom::Start(offset))?;
    let contents = match evl {
        Some(0) => Vec::new(),
        Some(lx) => {
            let mut contents = core::iter::repeat(0u8).take(lx).collect::<Vec<_>>();
            if lenspec.is_exact {
                fh.read_exact(&mut contents)?;
            } else {
                let bcnt = fh.read(&mut contents)?;
                contents.truncate(bcnt);
            }
            contents
        }
        None => {
            let mut contents = Vec::new();
            if let Err(x) = fh.read_to_end(&mut contents) {
                if lenspec.is_exact || contents.is_empty() {
                    return Err(x);
                }
            }
            contents
        }
    };
    Ok(Buffered(contents.into_boxed_slice()))
}

#[inline]
pub(crate) fn do_offset_add(offset: u64, x: i64) -> Option<u64> {
    if x < 0 {
        let xn = (-x) as u64;
        if xn <= offset {
            Some(offset - xn)
        } else {
            None
        }
    } else {
        Some(offset + (x as u64))
    }
}
