// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Print shares: [`PrintShare`], the backend of a share the library
//! serves with `Share::print`, over a [`Printer`] the job implements with
//! its spooler. Each CREATE opens a print job as the signed-on profile on
//! the share's output queue; its WRITEs are the data; its CLOSE ends the
//! spooled file.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use bytes::Bytes;
use smb_server::{
    BackendCapabilities, DirEntry, FileInfo, FileTimes, Handle, OpenAction, OpenIntent, OpenOptions,
    ShareBackend, SmbError, SmbPath, SmbResult,
};
use tracing::warn;

/// An output queue, `LIB/NAME`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutQueue {
    /// The library (`*LIBL` allowed).
    pub library: String,
    /// The queue.
    pub name: String,
}

/// Why printing failed.
#[derive(Debug, thiserror::Error)]
pub enum PrintError {
    /// The profile may not use the queue.
    #[error("not authorized to the output queue")]
    NotAuthorized,
    /// The queue does not exist.
    #[error("output queue not found")]
    NoQueue,
    /// Anything else, as the spooler said it.
    #[error("{0}")]
    Other(String),
}

/// A spooled file being written.
pub trait PrintJob: Send {
    /// Adds `data`.
    ///
    /// # Errors
    ///
    /// The spooler's.
    fn write(&mut self, data: &[u8]) -> Result<(), PrintError>;
    /// Ends the spooled file (it becomes ready on the queue).
    ///
    /// # Errors
    ///
    /// The spooler's.
    fn close(self: Box<Self>) -> Result<(), PrintError>;
}

/// The job's spooler. Blocking; keep each call short.
pub trait Printer: Send + Sync + 'static {
    /// Opens a spooled file for `user` on `queue`; `document` is the name
    /// the client gave the print file.
    ///
    /// # Errors
    ///
    /// Why it cannot be opened.
    fn open(&self, user: &str, queue: &OutQueue, document: &str) -> Result<Box<dyn PrintJob>, PrintError>;
}

/// The backend of one print share.
pub struct PrintShare {
    printer: Arc<dyn Printer>,
    queue: OutQueue,
    /// The profile signed on in the job.
    user: Arc<Mutex<Option<String>>>,
    next: AtomicU64,
}

impl PrintShare {
    /// A print share on `queue`, printing as the profile `user` holds.
    pub fn new(printer: Arc<dyn Printer>, queue: OutQueue, user: Arc<Mutex<Option<String>>>) -> Self {
        Self { printer, queue, user, next: AtomicU64::new(2) }
    }
}

fn smb(e: &PrintError) -> SmbError {
    match e {
        PrintError::NotAuthorized => SmbError::AccessDenied,
        PrintError::NoQueue => SmbError::PathNotFound,
        PrintError::Other(t) => SmbError::Io(std::io::Error::other(t.clone())),
    }
}

/// The share root (a directory) or one print file.
struct PrintHandle {
    name: String,
    index: u64,
    job: Mutex<Option<Box<dyn PrintJob>>>,
    written: AtomicU64,
}

#[async_trait]
impl ShareBackend for PrintShare {
    async fn open(&self, path: &SmbPath, opts: OpenOptions) -> SmbResult<(Box<dyn Handle>, OpenAction)> {
        if path.is_root() {
            let h = PrintHandle { name: String::new(), index: 1, job: Mutex::new(None), written: AtomicU64::new(0) };
            return Ok((Box::new(h), OpenAction::Opened));
        }
        // The library names every new print file; nothing else exists.
        if opts.intent != OpenIntent::Create {
            return Err(SmbError::NotFound);
        }
        let user = self.user.lock().unwrap_or_else(PoisonError::into_inner).clone().ok_or(SmbError::AccessDenied)?;
        let name = path.file_name().unwrap_or("PRINT").to_owned();
        let job = self.printer.open(&user, &self.queue, &name).map_err(|e| {
            warn!(user, queue = %self.queue.name, error = %e, "print file not opened");
            smb(&e)
        })?;
        let h = PrintHandle {
            name,
            index: self.next.fetch_add(1, Ordering::Relaxed),
            job: Mutex::new(Some(job)),
            written: AtomicU64::new(0),
        };
        Ok((Box::new(h), OpenAction::Created))
    }

    async fn unlink(&self, _: &SmbPath) -> SmbResult<()> {
        Err(SmbError::AccessDenied)
    }

    async fn rename(&self, _: &SmbPath, _: &SmbPath, _: bool) -> SmbResult<()> {
        Err(SmbError::AccessDenied)
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }
}

#[async_trait]
impl Handle for PrintHandle {
    async fn read(&self, _: u64, _: u32) -> SmbResult<Bytes> {
        Err(SmbError::AccessDenied)
    }

    async fn write(&self, _offset: u64, data: &[u8]) -> SmbResult<u32> {
        // Print data is a stream: each write adds to the spooled file.
        let mut job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        let job = job.as_mut().ok_or(SmbError::AccessDenied)?;
        job.write(data).map_err(|e| smb(&e))?;
        self.written.fetch_add(data.len() as u64, Ordering::Relaxed);
        Ok(u32::try_from(data.len()).unwrap_or(u32::MAX))
    }

    async fn flush(&self) -> SmbResult<()> {
        Ok(())
    }

    async fn stat(&self) -> SmbResult<FileInfo> {
        let mut info = FileInfo::new(self.name.clone());
        info.is_directory = self.index == 1;
        info.end_of_file = self.written.load(Ordering::Relaxed);
        info.allocation_size = info.end_of_file;
        info.file_index = self.index;
        Ok(info)
    }

    async fn set_times(&self, _: FileTimes) -> SmbResult<()> {
        Ok(())
    }

    async fn set_attributes(&self, _: u32) -> SmbResult<()> {
        Ok(())
    }

    async fn truncate(&self, len: u64) -> SmbResult<()> {
        // A new print file is empty; nothing else can be cut.
        if len == self.written.load(Ordering::Relaxed) { Ok(()) } else { Err(SmbError::AccessDenied) }
    }

    async fn list_dir(&self, _: Option<&str>) -> SmbResult<Vec<DirEntry>> {
        Ok(Vec::new())
    }

    async fn close(self: Box<Self>) -> SmbResult<()> {
        let job = self.job.lock().unwrap_or_else(PoisonError::into_inner).take();
        match job {
            Some(job) => job.close().map_err(|e| smb(&e)),
            None => Ok(()),
        }
    }
}

impl Drop for PrintHandle {
    fn drop(&mut self) {
        // A print file the library never closed still ends.
        if let Some(job) = self.job.lock().unwrap_or_else(PoisonError::into_inner).take() {
            let _ = job.close();
        }
    }
}
