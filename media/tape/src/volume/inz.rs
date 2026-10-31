// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! INZTAP: initialize a volume.

use super::scan::{VolumeKind, identify, scan_files};
use super::{EndOpt, check_volume_id};
use crate::TapeError;
use crate::label::{Code, Date, Vol1};
use crate::transport::TapeTransport;

/// INZTAP CHECK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Check {
    /// CHECK(*YES): refuse if any file on the volume is active.
    #[default]
    Yes,
    /// CHECK(*NO): do not read the volume first.
    No,
    /// CHECK(*FIRST): refuse if the first file is active.
    First,
}

/// INZTAP's parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InzOptions {
    /// NEWVOL: the new volume identifier, or `None` for NEWVOL(*NONE),
    /// which leaves the volume unlabeled.
    pub new_vol: Option<String>,
    /// NEWOWNID: up to 14 characters.
    pub new_owner: String,
    /// VOL: the volume that must be mounted, or `None` for *MOUNTED.
    pub vol: Option<String>,
    /// CHECK.
    pub check: Check,
    /// DENSITY: `None` for *DEVTYPE (keep the drive's).
    pub density: Option<String>,
    /// CODE.
    pub code: Code,
    /// ENDOPT.
    pub end_opt: EndOpt,
    /// CLEAR(*YES): erase the whole volume.
    pub clear: bool,
    /// Today, for the active-file check.
    pub today: Option<Date>,
}

/// Refuse while any of `files` (as far as `first_only` says) is active.
pub(super) fn refuse_active(
    volume: &str,
    files: &[super::FileInfo],
    first_only: bool,
    today: Option<Date>,
) -> Result<(), TapeError> {
    let Some(today) = today else { return Ok(()) };
    let n = if first_only { 1 } else { files.len() };
    for f in files.iter().take(n) {
        if f.label.expires.active(today) {
            return Err(TapeError::ActiveFile {
                volume: volume.to_string(),
                seq: f.label.file_seq,
                label: f.label.file_id.clone(),
                expires: crate::error::expiry_text(match f.label.expires {
                    crate::label::Expiry::Date(d) => Some(d),
                    _ => None,
                }),
            });
        }
    }
    Ok(())
}

/// Initialize the mounted volume.
pub fn inztap<D: TapeTransport + ?Sized>(t: &mut D, opts: &InzOptions) -> Result<(), TapeError> {
    if let Some(v) = &opts.new_vol {
        check_volume_id("NEWVOL", v)?;
    }
    if opts.new_owner.len() > 14 {
        return Err(TapeError::Parameter {
            param: "NEWOWNID",
            why: "longer than 14 characters".into(),
        });
    }
    if let Some(v) = &opts.vol {
        check_volume_id("VOL", v)?;
    }
    let status = t.status()?;
    if !status.loaded {
        return Err(crate::TransportError::NotLoaded.into());
    }
    if status.write_protected {
        return Err(crate::TransportError::WriteProtected.into());
    }
    if opts.check != Check::No {
        let kind = identify(t)?;
        if let Some(want) = &opts.vol
            && &kind.id() != want
        {
            return Err(TapeError::WrongVolume {
                expected: want.clone(),
                found: kind.id(),
            });
        }
        if let VolumeKind::Labeled { vol1, code } = &kind {
            let files = scan_files(t, *code)?;
            refuse_active(
                &vol1.volume_id,
                &files,
                opts.check == Check::First,
                opts.today,
            )?;
        }
    }
    t.rewind()?;
    t.set_block_size(0)?;
    if let Some(d) = &opts.density {
        t.set_density(d)?;
    }
    if opts.clear {
        t.erase()?;
        t.rewind()?;
    }
    if let Some(id) = &opts.new_vol {
        let vol1 = Vol1 {
            volume_id: id.clone(),
            owner: opts.new_owner.clone(),
        };
        t.write_block(&vol1.encode(opts.code)?)?;
    }
    t.write_eof(2)?;
    t.rewind()?;
    opts.end_opt.apply(t)
}
