mod backend;

use crate::backend::*;
use delegate_attr::delegate;
use memmap2::Mmap;
use std::{fs, io, ops::Deref};

// public interface

/// buffered or mmapped file contents handle
#[must_use]
pub enum FileHandle {
    Mapped(Mmap),
    Buffered(Box<[u8]>),
}

use self::FileHandle::*;

impl FileHandle {
    /// This function returns a slice pointing to
    /// the contents of the [`FileHandle`].
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Mapped(ref dt) => dt,
            Buffered(ref dt) => dt,
        }
    }
}

impl AsRef<[u8]> for FileHandle {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Deref for FileHandle {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct LengthSpec {
    /// `bound` ? (read at most $n bytes) : (read until EOF)
    pub bound: Option<usize>,
    /// `is_exact` ? (request exactly length or fail) : (request biggest readable slice with length as upper bound)
    pub is_exact: bool,
}

impl std::default::Default for LengthSpec {
    /// read as much as possible
    #[inline(always)]
    fn default() -> Self {
        Self {
            bound: None,
            is_exact: false,
        }
    }
}

/// Returns the length of the file,
/// and is based upon [`memmap2::MmapOptions::get_len()`].
/// It doesn't sanitize the fact that mapping a slice greater than isize::MAX
/// has undefined behavoir.
pub fn get_file_len(fh: &fs::File) -> Option<u64> {
    fh.metadata().ok().map(|x| x.len())
}

/// Reads the file contents
pub fn read_from_file(fh: io::Result<fs::File>) -> io::Result<FileHandle> {
    read_part_from_file(
        &mut fh?,
        0,
        LengthSpec {
            bound: None,
            is_exact: true,
        },
    )
}

/// Reads a part of the file contents,
/// use this if the file is too big and needs to be read in parts,
/// starting at offset and until the given LengthSpec is met.
/// if you want a more ergonomic interface, use [`ContinuableFile`] or [`ChunkedFile`].
/// fh is a reference because this function is intended to be called multiple times
#[inline]
pub fn read_part_from_file(
    fh: &mut fs::File,
    offset: u64,
    len: LengthSpec,
) -> io::Result<FileHandle> {
    read_part_from_file_intern(fh, offset, len, None)
}

#[must_use]
pub struct ContinuableFile {
    file: fs::File,
    flen: Option<u64>,
    offset: u64,
}

#[must_use]
pub struct ChunkedFile {
    pub cf: ContinuableFile,
    pub lns: LengthSpec,
}

impl ContinuableFile {
    pub fn new(file: fs::File) -> Self {
        let mut ret = Self {
            file,
            flen: None,
            offset: 0,
        };
        ret.sync_len();
        ret
    }

    #[inline]
    pub fn into_chunks(self, lns: LengthSpec) -> ChunkedFile {
        ChunkedFile { cf: self, lns }
    }

    #[inline]
    pub fn sync_len(&mut self) {
        self.flen = get_file_len(&self.file);
    }

    /// Tries to read the next part of the file contents, according to the LengthSpec
    pub fn next(&mut self, lns: LengthSpec) -> io::Result<FileHandle> {
        let rfh = read_part_from_file_intern(&mut self.file, self.offset, lns, self.flen)?;
        self.offset += rfh.len() as u64;
        Ok(rfh)
    }

    fn get_soor_err() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, "seek out of range")
    }
}

impl io::Seek for ContinuableFile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        use io::SeekFrom::*;

        if let Some(y) = match pos {
            Start(x) => Some(x),
            End(x) => self.flen.and_then(|flen| do_offset_add(flen, x)),
            Current(x) => do_offset_add(self.offset, x),
        } {
            if self.flen.map(|flen| flen < y) != Some(true) {
                self.offset = y;
                return Ok(y);
            }
        }
        Err(Self::get_soor_err())
    }

    //#[inline(always)]
    //fn stream_len(&mut self) -> io::Result<u64> {
    //    self.flen.ok_or_else(Self::get_soor_err)
    //}

    #[inline(always)]
    fn stream_position(&mut self) -> io::Result<u64> {
        Ok(self.offset)
    }
}

impl std::iter::Iterator for ChunkedFile {
    type Item = io::Result<FileHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.cf.next(self.lns) {
            Ok(ref x) if x.is_empty() => None,
            item => Some(item),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (flen, offset) = (self.cf.flen, self.cf.offset);
        let lower_bound =
            flen.and_then(|x| self.lns.bound.map(|y| ((x - offset) as usize) / y));
        (lower_bound.unwrap_or(0), lower_bound.map(|x| x + 1))
    }
}

#[delegate(self.cf)]
impl io::Seek for ChunkedFile {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {}

    //#[cfg(feature = "seek_stream_len")]
    //fn stream_len(&mut self) -> io::Result<u64> {}

    fn stream_position(&mut self) -> io::Result<u64> {}
}
