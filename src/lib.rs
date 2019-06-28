#![cfg_attr(feature = "seek_convenience", feature(seek_convenience))]

extern crate boolinator;
extern crate memmap;
mod backend;

use backend::*;
use memmap::Mmap;
use std::{fs, io};

// public interface

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

impl std::ops::Deref for FileHandle {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.get_slice()
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct LengthSpec {
    bound: Option<usize>,
    is_exact: bool,
}

impl LengthSpec {
    /// @param bound
    ///   ? (read at most $n bytes)
    ///   : (read until EOF)
    /// @param is_exact
    ///   ? (request exactly length or fail)
    ///   : (request biggest readable slice with length as upper bound)
    pub fn new(bound: Option<usize>, is_exact: bool) -> Self {
        Self { bound, is_exact }
    }
}

impl std::default::Default for LengthSpec {
    /// read as much as possible
    #[inline]
    fn default() -> Self {
        Self {
            bound: None,
            is_exact: false,
        }
    }
}

/// Returns the length of the file,
/// and is based upon [`memmap::MmapOptions::get_len()`].
/// It doesn't sanitize the fact that mapping a slice greater than isize::MAX
/// has undefined behavoir.
pub fn get_file_len(fh: &fs::File) -> Option<u64> {
    fh.metadata().ok().map(|x| x.len())
}

/// Reads the file contents
pub fn read_from_file(fh: io::Result<fs::File>) -> io::Result<FileHandle> {
    let mut fh = fh?;
    let lns = LengthSpec {
        bound: None,
        is_exact: true,
    };
    read_part_from_file(&mut fh, 0, lns)
}

/// Reads a part of the file contents,
/// use this if the file is too big and needs to be read in parts,
/// starting at offset and until the given LengthSpec is met.
/// if you want a more ergonomic interface, use [`ContinuableFile`] or [`ChunkedFile`].
/// fh is a reference because this function is intended to be called multiple times
#[inline]
pub fn read_part_from_file(
    mut fh: &mut fs::File,
    offset: u64,
    len: LengthSpec,
) -> io::Result<FileHandle> {
    read_part_from_file_intern(&mut fh, offset, len, None)
}

#[must_use]
pub struct ContinuableFile {
    file: fs::File,
    flen: Option<u64>,
    offset: u64,
}

#[must_use]
pub struct ChunkedFile {
    cf: ContinuableFile,
    lns: LengthSpec,
}

impl ContinuableFile {
    pub fn new(file: fs::File) -> Self {
        let mut ret = Self {
            file,
            flen: None,
            offset: 0,
        };
        ret.sync_len();
        return ret;
    }

    pub fn to_chunks(self, lns: LengthSpec) -> ChunkedFile {
        ChunkedFile { cf: self, lns }
    }

    pub fn sync_len(&mut self) {
        self.flen = get_file_len(&self.file);
    }

    /// Tries to read the next part of the file contents
    pub fn next(&mut self, lns: LengthSpec) -> io::Result<FileHandle> {
        let rfh = read_part_from_file_intern(&mut self.file, self.offset, lns, self.flen)?;
        self.offset += rfh.len() as u64;
        Ok(rfh)
    }
}

impl io::Seek for ContinuableFile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let oore = Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "seek out of range",
        ));
        use io::SeekFrom::*;

        match pos {
            Start(x) => self.offset = x,
            End(x) => {
                let xn: u64 = (-x) as u64;
                if (x > 0) || self.flen.is_none() || (xn > self.flen.unwrap()) {
                    return oore;
                }
                self.offset = self.flen.unwrap() - xn;
            }
            Current(x) => match do_offset_add(self.offset, x) {
                Some(y) => self.offset = y,
                None => return oore,
            },
        }

        Ok(self.offset)
    }

    #[cfg(feature = "seek_convenience")]
    fn stream_len(&mut self) -> io::Result<u64> {
        match self.flen {
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek out of range",
            )),
            Some(x) => Ok(x),
        }
    }

    #[cfg(feature = "seek_convenience")]
    fn stream_position(&mut self) -> io::Result<u64> {
        Ok(self.offset)
    }
}

// getters
impl ChunkedFile {
    pub fn get_inner_ref(&mut self) -> &mut ContinuableFile {
        &mut self.cf
    }

    pub fn into_inner(self) -> ContinuableFile {
        self.cf
    }

    pub fn get_lns(&self) -> LengthSpec {
        self.lns
    }
}

impl std::iter::Iterator for ChunkedFile {
    type Item = io::Result<FileHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.cf.next(self.lns);
        match item {
            Ok(ref x) if x.len() == 0 => None,
            _ => Some(item),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let flen = self.cf.flen;
        (
            flen.map(|x| (x - self.cf.offset as u64) as usize)
                .unwrap_or(0),
            None,
        )
    }
}

impl io::Seek for ChunkedFile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.cf.seek(pos)
    }
}
