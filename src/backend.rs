use crate::{get_file_len, FileHandle, FileHandle::*, LengthSpec};
use boolinator::Boolinator;
use std::{fs::File, io, io::Read, io::Seek};

// private interface

fn open_as_mmap(fh: &File, offset: u64, len: usize) -> io::Result<memmap::Mmap> {
    Ok(unsafe {
        // NOTE: replace map_copy?.make_read_only? with map_copy_read_only?
        // once issue danburkert/memmap-rs#81 is fixed
        memmap::MmapOptions::new()
            .offset(offset)
            .len(len)
            .map_copy(&fh)?
            .make_read_only()?
    })
}

#[must_use]
enum EvaluatedLength {
    ELUnknown,
    ELImpossible,
    ELBounded(usize),
}

use self::EvaluatedLength::*;

/// @param flen_hint : used to cache the call to [`get_file_len`]
fn eval_length(
    fh: &File,
    offset: u64,
    lenspec: LengthSpec,
    flen_hint: Option<u64>,
) -> EvaluatedLength {
    let maxlen_i = std::isize::MAX as usize;
    let is_untileof = lenspec.bound.is_none();
    let x = lenspec.bound.unwrap_or(maxlen_i);
    let maxlen_f = flen_hint
        .or_else(|| get_file_len(&fh))
        .map(|lx| (lx - offset) as usize)
        .unwrap_or(maxlen_i);
    let maxlen = std::cmp::min(maxlen_f, maxlen_i);
    return if x > maxlen {
        // ensure maximum length
        if !lenspec.is_exact {
            ELBounded(maxlen)
        } else if !is_untileof {
            ELImpossible
        } else if maxlen == maxlen_i {
            ELUnknown
        } else {
            ELBounded(maxlen)
        }
    } else {
        if is_untileof {
            ELUnknown
        } else {
            ELBounded(x)
        }
    };
}

/// Reads a part of the file contents,
/// use this if the file is too big and needs to be read in parts,
/// starting at [`offset`] and until the given LengthSpec is met.
pub(crate) fn read_part_from_file_intern(
    fh: &mut File,
    offset: u64,
    len: LengthSpec,
    flen_hint: Option<u64>,
) -> io::Result<FileHandle> {
    let evl = eval_length(&fh, offset, len, flen_hint);
    match evl {
        ELImpossible => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "length is too big",
            ));
        }
        ELBounded(0) => {
            return Ok(Buffered(Vec::new()));
        }
        ELBounded(lx) => {
            // do NOT try to map the file if the size is unknown
            if let Some(ret) = open_as_mmap(&fh, offset, lx).ok() {
                return Ok(Mapped(ret));
            }
        }
        ELUnknown => {}
    }

    // use Buffered
    use std::hint::unreachable_unchecked;
    fh.seek(io::SeekFrom::Start(offset))?;
    let mut bfr = io::BufReader::new(fh);
    let mut contents = Vec::new();
    match evl {
        ELImpossible => unsafe { unreachable_unchecked() },
        ELBounded(lx) => {
            contents.resize(lx, 0);
            if len.is_exact {
                bfr.read_exact(&mut contents)?;
            } else {
                bfr.read(&mut contents)?;
            }
        }
        ELUnknown => {
            if let Err(x) = bfr.read_to_end(&mut contents) {
                if len.is_exact || contents.is_empty() {
                    return Err(x);
                }
            }
        }
    };
    contents.shrink_to_fit();
    Ok(Buffered(contents))
}

pub(crate) fn do_offset_add(offset: u64, x: i64) -> Option<u64> {
    if x < 0 {
        let xn = (-x) as u64;
        (xn <= offset).as_some_from(|| offset - xn)
    } else {
        Some(offset + (x as u64))
    }
}
