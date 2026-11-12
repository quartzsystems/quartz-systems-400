// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where the users are kept between runs.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

/// A place that keeps the users' bytes.
pub trait UserStore {
    /// What reading or writing it can fail with.
    type Error;
    /// The bytes kept, if any.
    fn read(&mut self) -> Result<Option<Vec<u8>>, Self::Error>;
    /// Keep these bytes in place of the ones before, durably.
    fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

/// The users in a file of their own: written beside it, flushed, and
/// renamed over it, so a crash leaves the users of before or of after.
#[derive(Debug, Clone)]
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    /// The file; it need not exist yet.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl UserStore for FileStore {
    type Error = io::Error;

    fn read(&mut self) -> io::Result<Option<Vec<u8>>> {
        match fs::read(&self.path) {
            Ok(b) => Ok(Some(b)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".new");
        let mut f = File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &self.path)?;
        // The rename is durable once the directory is.
        if let Some(dir) = self.path.parent()
            && let Ok(d) = File::open(dir)
        {
            let _ = d.sync_all();
        }
        Ok(())
    }
}
