// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where image bytes live: [`ReadAt`] for reading at offsets,
//! [`ImageStore`] for images that can be rewritten whole, and the
//! in-memory, file and window forms of both.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

/// Bytes readable at any offset.
pub trait ReadAt: Send + Sync {
    /// Length in bytes.
    fn size(&self) -> io::Result<u64>;
    /// Read at `off`; fewer bytes than asked only at the end.
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize>;

    /// Fill `buf` from `off` or fail.
    fn read_exact_at(&self, off: u64, buf: &mut [u8]) -> io::Result<()> {
        let mut done = 0;
        while done < buf.len() {
            let n = self.read_at(off + done as u64, &mut buf[done..])?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "read past the end of the image",
                ));
            }
            done += n;
        }
        Ok(())
    }
}

impl ReadAt for [u8] {
    fn size(&self) -> io::Result<u64> {
        Ok(self.len() as u64)
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        let Ok(off) = usize::try_from(off) else {
            return Ok(0);
        };
        if off >= self.len() {
            return Ok(0);
        }
        let n = buf.len().min(self.len() - off);
        buf[..n].copy_from_slice(&self[off..off + n]);
        Ok(n)
    }
}

impl ReadAt for Vec<u8> {
    fn size(&self) -> io::Result<u64> {
        self.as_slice().size()
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.as_slice().read_at(off, buf)
    }
}

impl<T: ReadAt + ?Sized> ReadAt for &T {
    fn size(&self) -> io::Result<u64> {
        (**self).size()
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        (**self).read_at(off, buf)
    }
}

impl<T: ReadAt + ?Sized> ReadAt for Arc<T> {
    fn size(&self) -> io::Result<u64> {
        (**self).size()
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        (**self).read_at(off, buf)
    }
}

impl<T: ReadAt + ?Sized> ReadAt for Box<T> {
    fn size(&self) -> io::Result<u64> {
        (**self).size()
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        (**self).read_at(off, buf)
    }
}

impl ReadAt for File {
    fn size(&self) -> io::Result<u64> {
        let len = self.metadata()?.len();
        if len > 0 {
            return Ok(len);
        }
        // Block devices report zero; seek to the end instead.
        let mut f = self;
        io::Seek::seek(&mut f, io::SeekFrom::End(0))
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        #[cfg(unix)]
        {
            std::os::unix::fs::FileExt::read_at(self, buf, off)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::FileExt::seek_read(self, buf, off)
        }
    }
}

/// A window of another source: `len` bytes from `start`.
#[derive(Debug, Clone)]
pub struct Slice<R> {
    inner: R,
    start: u64,
    len: u64,
}

impl<R: ReadAt> Slice<R> {
    /// The window.
    pub fn new(inner: R, start: u64, len: u64) -> Self {
        Self { inner, start, len }
    }
    /// The source under it.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: ReadAt> ReadAt for Slice<R> {
    fn size(&self) -> io::Result<u64> {
        Ok(self.len)
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        if off >= self.len {
            return Ok(0);
        }
        let n = usize::try_from((self.len - off).min(buf.len() as u64)).unwrap_or(buf.len());
        self.inner.read_at(self.start + off, &mut buf[..n])
    }
}

/// An image that can be replaced whole: the backing of a writable
/// volume. `replace` must keep the old bytes readable while `write`
/// runs, since the new image copies unchanged files from the old one.
pub trait ImageStore: ReadAt {
    /// Write a new image through `write`, then make it the image.
    fn replace(&self, write: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<()>;
}

impl<T: ImageStore + ?Sized> ImageStore for Arc<T> {
    fn replace(&self, write: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
        (**self).replace(write)
    }
}

/// An image in memory.
#[derive(Debug, Default)]
pub struct MemImage {
    bytes: RwLock<Vec<u8>>,
}

impl MemImage {
    /// An image holding `bytes`.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: RwLock::new(bytes),
        }
    }
    /// A copy of the bytes.
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.read().map(|b| b.clone()).unwrap_or_default()
    }
}

impl ReadAt for MemImage {
    fn size(&self) -> io::Result<u64> {
        Ok(self.bytes.read().map_or(0, |b| b.len() as u64))
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        let b = self
            .bytes
            .read()
            .map_err(|_| io::Error::other("image lock poisoned"))?;
        b.as_slice().read_at(off, buf)
    }
}

impl ImageStore for MemImage {
    fn replace(&self, write: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
        let mut next = Vec::new();
        write(&mut next)?;
        *self
            .bytes
            .write()
            .map_err(|_| io::Error::other("image lock poisoned"))? = next;
        Ok(())
    }
}

/// An image file. Replacing writes `NAME.new` beside it and renames.
#[derive(Debug)]
pub struct FileImage {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl FileImage {
    /// An existing image file.
    pub fn open(path: &Path) -> io::Result<Self> {
        let f = File::open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            file: Mutex::new(Some(f)),
        })
    }

    /// A new file holding `bytes` (it must not exist).
    pub fn create(path: &Path, bytes: &[u8]) -> io::Result<Self> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        Self::open(path)
    }

    /// The file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn with<T>(&self, f: impl FnOnce(&File) -> io::Result<T>) -> io::Result<T> {
        let mut g = self
            .file
            .lock()
            .map_err(|_| io::Error::other("image lock poisoned"))?;
        if g.is_none() {
            *g = Some(File::open(&self.path)?);
        }
        f(g.as_ref().expect("opened above"))
    }
}

impl ReadAt for FileImage {
    fn size(&self) -> io::Result<u64> {
        self.with(ReadAt::size)
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.with(|f| ReadAt::read_at(f, off, buf))
    }
}

impl ImageStore for FileImage {
    fn replace(&self, write: &mut dyn FnMut(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".new");
        let tmp = PathBuf::from(tmp);
        let out = File::create(&tmp)?;
        let mut w = io::BufWriter::new(out);
        if let Err(e) = write(&mut w) {
            drop(w);
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        let out = w.into_inner().map_err(io::IntoInnerError::into_error)?;
        out.sync_all()?;
        drop(out);
        // Windows will not rename over an open file.
        let mut g = self
            .file
            .lock()
            .map_err(|_| io::Error::other("image lock poisoned"))?;
        *g = None;
        fs::rename(&tmp, &self.path)?;
        *g = Some(File::open(&self.path)?);
        Ok(())
    }
}
