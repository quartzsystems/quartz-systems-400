// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A save as one file on an optical volume: `OPTFILE('*')` is the
//! generated name (`/MYLIB`, `/QSAVIFS.nnn`, `/QSAVSYS/<KIND>`),
//! `OPTFILE('dir/*')` the generated name in that directory, anything
//! else the path itself.

use optical::VolumeSet;
use optical::volume::layout::{SaveTarget, generated_save_path};

use crate::{MediaError, MediaReader, MediaWriter, Result, Written};

/// The path `OPTFILE` names for a save of `target`.
#[must_use]
pub fn save_path(optfile: &str, target: SaveTarget<'_>) -> String {
    let f = optfile.trim().trim_matches('\'');
    let generated = generated_save_path(target);
    if f.is_empty() || f == "*" {
        return generated;
    }
    let path = if f.starts_with('/') {
        f.to_string()
    } else {
        format!("/{f}")
    };
    match path.strip_suffix("/*") {
        Some(dir) => {
            let name = generated.rsplit('/').next().unwrap_or(&generated);
            format!("{dir}/{name}")
        }
        None => path,
    }
}

/// Writes a save to a file on a loaded volume.
pub struct OpticalWriter<'a> {
    set: &'a VolumeSet,
    volume: String,
    path: String,
    replace: bool,
}

impl<'a> OpticalWriter<'a> {
    /// A writer of `path` on `volume`; `replace` lets an existing file
    /// go (`CLEAR(*ALL)` or `*REPLACE`).
    #[must_use]
    pub fn new(set: &'a VolumeSet, volume: &str, path: &str, replace: bool) -> Self {
        Self {
            set,
            volume: volume.to_string(),
            path: path.to_string(),
            replace,
        }
    }
}

/// Each directory above `path`, made where missing.
fn make_parents(v: &mut optical::Volume, path: &str) -> optical::Result<()> {
    let mut at = String::new();
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    for p in parts.iter().take(parts.len().saturating_sub(1)) {
        at.push('/');
        at.push_str(p);
        if v.stat(&at).is_err() {
            v.mkdir(&at)?;
        }
    }
    Ok(())
}

impl MediaWriter for OpticalWriter<'_> {
    fn write_stream(&mut self, data: &[u8]) -> Result<Written> {
        let (path, replace) = (self.path.clone(), self.replace);
        let exists = self.set.with(&self.volume, |v| Ok(v.stat(&path).is_ok()))?;
        if exists && !replace {
            return Err(MediaError::Exists(path));
        }
        self.set.with(&self.volume, |v| {
            make_parents(v, &path)?;
            v.put_file(&path, data.to_vec(), true)
        })?;
        self.set.sync(&self.volume)?;
        Ok(Written {
            seq: 0,
            volumes: vec![self.volume.clone()],
            file: self.path.clone(),
            bytes: data.len() as u64,
        })
    }
}

/// Reads a save from a file on a loaded volume.
pub struct OpticalReader<'a> {
    set: &'a VolumeSet,
    volume: String,
    path: String,
}

impl<'a> OpticalReader<'a> {
    /// A reader of `path` on `volume`.
    #[must_use]
    pub fn new(set: &'a VolumeSet, volume: &str, path: &str) -> Self {
        Self {
            set,
            volume: volume.to_string(),
            path: path.to_string(),
        }
    }
}

impl MediaReader for OpticalReader<'_> {
    fn read_stream(&mut self) -> Result<(Vec<u8>, Vec<String>)> {
        let path = self.path.clone();
        let data = self.set.with(&self.volume, |v| {
            if v.stat(&path).is_err() {
                return Err(optical::Error::NotFound(path.clone()));
            }
            v.read_all(&path)
        })?;
        Ok((data, vec![self.volume.clone()]))
    }
}
