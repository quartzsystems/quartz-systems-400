// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The run directory every writing scenario uses, and removing things.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use smb::{FileAccessMask, FileCreateArgs, FileDirectoryInformation, FileDispositionInformation};

use super::Lines;
use crate::conn::{Share, base_config};
use crate::{Error, Target};

/// `SCRATCH\smbtest-PID-MILLIS` on the share.
pub struct Scratch {
    target: Target,
    /// The run directory, relative to the share.
    pub dir: String,
}

impl Scratch {
    /// A new run directory name (nothing is made yet).
    pub fn new(t: &Target) -> Scratch {
        let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let leaf = format!("smbtest-{}-{millis}", std::process::id());
        let dir = if t.scratch.is_empty() { leaf } else { format!("{}\\{leaf}", t.scratch) };
        Scratch { target: t.clone(), dir }
    }

    /// `name` inside the run directory.
    pub fn file(&self, name: &str) -> String {
        format!("{}\\{name}", self.dir)
    }

    /// Makes the run directory.
    pub async fn create(&self) -> Lines {
        let s = Share::connect(&self.target, base_config(&self.target)).await?;
        s.create_dir(&self.dir).await?;
        s.close().await?;
        Ok(vec![format!("directory: {}", self.dir)])
    }

    /// Removes the run directory and whatever is left in it.
    pub async fn remove(&self) -> Lines {
        let s = Share::connect(&self.target, base_config(&self.target)).await?;
        let left = list(&s, &self.dir).await?;
        for name in &left {
            remove(&s, &self.file(name)).await?;
        }
        remove(&s, &self.dir).await?;
        s.close().await?;
        Ok(vec![format!("left over: {}", if left.is_empty() { "none".into() } else { left.join(", ") })])
    }
}

/// The names in a directory, without `.` and `..`.
pub async fn list(s: &Share, dir: &str) -> Result<Vec<String>, Error> {
    let d = Arc::new(s.open_dir(dir).await?);
    let mut names = Vec::new();
    {
        let mut stream = smb::Directory::query::<FileDirectoryInformation>(&d, "*").await?;
        while let Some(e) = stream.next().await {
            let name = e?.file_name.to_string();
            if name != "." && name != ".." {
                names.push(name);
            }
        }
    }
    d.close().await?;
    Ok(names)
}

/// Deletes a file or an empty directory: delete on close, then close.
pub async fn remove(s: &Share, path: &str) -> Result<(), Error> {
    let access = FileAccessMask::new().with_delete(true).with_file_read_attributes(true);
    let r = s.open(path, &FileCreateArgs::make_open_existing(access)).await?;
    let h = match &r {
        smb::Resource::File(f) => f.handle(),
        smb::Resource::Directory(d) => d.handle(),
        smb::Resource::Pipe(_) => return Err(Error::Check(format!("{path} is a pipe"))),
    };
    h.set_info(FileDispositionInformation { delete_pending: true.into() }).await?;
    h.close().await?;
    Ok(())
}
