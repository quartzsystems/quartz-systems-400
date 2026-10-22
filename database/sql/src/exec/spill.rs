// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where a sort puts the runs it cannot hold: the job's temporary
//! storage. `MemSpill` keeps them in memory, which is what the
//! temporary ASP is on this system today; `FileSpill` writes them to a
//! directory for a host whose temporary storage is a file system.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use crate::error::{Result, SqlError};

/// A run of sorted rows written once and read back in order.
pub type RunId = usize;

/// Temporary storage for sort runs.
pub trait SpillArea {
    /// Keep `bytes` as a new run; its identifier.
    fn write_run(&mut self, bytes: &[u8]) -> Result<RunId>;
    /// Up to `len` bytes of a run from `off`; fewer at its end.
    fn read(&mut self, run: RunId, off: u64, len: usize) -> Result<Vec<u8>>;
    /// Every run gone.
    fn clear(&mut self);
}

/// Runs held in memory.
#[derive(Default)]
pub struct MemSpill {
    runs: Vec<Vec<u8>>,
}

impl SpillArea for MemSpill {
    fn write_run(&mut self, bytes: &[u8]) -> Result<RunId> {
        self.runs.push(bytes.to_vec());
        Ok(self.runs.len() - 1)
    }

    fn read(&mut self, run: RunId, off: u64, len: usize) -> Result<Vec<u8>> {
        let r = self.runs.get(run).ok_or_else(|| SqlError::Database {
            what: format!("sort run {run} missing"),
            msgid: None,
        })?;
        let off = usize::try_from(off).unwrap_or(usize::MAX).min(r.len());
        let end = off.saturating_add(len).min(r.len());
        Ok(r[off..end].to_vec())
    }

    fn clear(&mut self) {
        self.runs.clear();
    }
}

/// Runs as files in a directory, one per run, removed on `clear`.
pub struct FileSpill {
    dir: PathBuf,
    runs: Vec<(PathBuf, File)>,
    seq: u64,
}

impl FileSpill {
    /// Spill into `dir`, which must exist.
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            runs: Vec::new(),
            seq: 0,
        }
    }
}

fn io(e: &std::io::Error) -> SqlError {
    SqlError::Database {
        what: format!("sort spill: {e}"),
        msgid: None,
    }
}

impl SpillArea for FileSpill {
    fn write_run(&mut self, bytes: &[u8]) -> Result<RunId> {
        self.seq += 1;
        let path = self
            .dir
            .join(format!("qsql-{}-{}.run", std::process::id(), self.seq));
        let mut f = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| io(&e))?;
        f.write_all(bytes).map_err(|e| io(&e))?;
        self.runs.push((path, f));
        Ok(self.runs.len() - 1)
    }

    fn read(&mut self, run: RunId, off: u64, len: usize) -> Result<Vec<u8>> {
        let (_, f) = self.runs.get_mut(run).ok_or_else(|| SqlError::Database {
            what: format!("sort run {run} missing"),
            msgid: None,
        })?;
        f.seek(SeekFrom::Start(off)).map_err(|e| io(&e))?;
        let mut buf = vec![0u8; len];
        let mut got = 0;
        while got < len {
            let n = f.read(&mut buf[got..]).map_err(|e| io(&e))?;
            if n == 0 {
                break;
            }
            got += n;
        }
        buf.truncate(got);
        Ok(buf)
    }

    fn clear(&mut self) {
        for (path, f) in self.runs.drain(..) {
            drop(f);
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Drop for FileSpill {
    fn drop(&mut self) {
        self.clear();
    }
}
