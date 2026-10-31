// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! CHKTAP: check the volume and, optionally, find a file on it.

use super::scan::{FileInfo, VolumeKind, identify, scan_files, seek_end, seek_header};
use super::{EndOpt, SeqNbr};
use crate::TapeError;
use crate::transport::TapeTransport;

/// CHKTAP's parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChkOptions {
    /// VOL: `None` for *MOUNTED.
    pub vol: Option<String>,
    /// SEQNBR: `None` for *ONLY (the label alone finds the file, or no
    /// file is looked for).
    pub seq: Option<SeqNbr>,
    /// FILELABEL: `None` for *ALL.
    pub file_label: Option<String>,
    /// ENDOPT.
    pub end_opt: EndOpt,
}

/// What CHKTAP found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChkResult {
    /// The volume.
    pub kind: VolumeKind,
    /// The file found, when one was looked for.
    pub file: Option<FileInfo>,
}

/// Check the mounted volume. With ENDOPT(*LEAVE) the drive is left at
/// the file's HDR1 (or where SEQNBR(*END) would write).
pub fn chktap<D: TapeTransport + ?Sized>(
    t: &mut D,
    opts: &ChkOptions,
) -> Result<ChkResult, TapeError> {
    let kind = identify(t)?;
    if let Some(want) = &opts.vol
        && &kind.id() != want
    {
        return Err(TapeError::WrongVolume {
            expected: want.clone(),
            found: kind.id(),
        });
    }
    let looking = opts.seq.is_some() || opts.file_label.is_some();
    let mut file = None;
    if looking {
        let VolumeKind::Labeled { code, .. } = &kind else {
            return Err(TapeError::NotLabeled);
        };
        let files = scan_files(t, *code)?;
        if opts.seq == Some(SeqNbr::End) {
            seek_end(t, &files)?;
        } else {
            let seq = match opts.seq {
                Some(SeqNbr::Number(n)) => Some(n),
                _ => None,
            };
            let found = files.into_iter().find(|f| {
                seq.is_none_or(|s| f.label.file_seq == s)
                    && opts
                        .file_label
                        .as_ref()
                        .is_none_or(|l| &f.label.file_id == l)
            });
            let Some(f) = found else {
                let what = match (seq, &opts.file_label) {
                    (Some(s), _) => s.to_string(),
                    (None, Some(l)) => l.clone(),
                    (None, None) => String::new(),
                };
                return Err(TapeError::FileNotFound { what });
            };
            seek_header(t, f.index)?;
            file = Some(f);
        }
    }
    opts.end_opt.apply(t)?;
    Ok(ChkResult { kind, file })
}
