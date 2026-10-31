// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading a volume's labels: what it is, and every file on it (the
//! data DSPTAP DATA(*LABELS) shows).

use super::EndOpt;
use crate::TapeError;
use crate::label::layout::LABEL_LEN;
use crate::label::{Code, FileLabel, LabelError, LabelSet, Vol1};
use crate::transport::{MAX_BLOCK, ReadOutcome, TapeTransport, TransportError};

/// What is on a volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeKind {
    /// Standard labels.
    Labeled {
        /// The volume label.
        vol1: Vol1,
        /// The code it is written in.
        code: Code,
    },
    /// Data without labels (*NL), or anything else that is not VOL1.
    Unlabeled,
    /// Nothing recorded.
    Blank,
}

impl VolumeKind {
    /// The volume identifier, "*NL" or "*BLANK".
    #[must_use]
    pub fn id(&self) -> String {
        match self {
            Self::Labeled { vol1, .. } => vol1.volume_id.clone(),
            Self::Unlabeled => "*NL".into(),
            Self::Blank => "*BLANK".into(),
        }
    }
}

/// One file of a labeled volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// The header labels, with the trailer's block count.
    pub label: FileLabel,
    /// How the file ends on this volume: [`LabelSet::EndOfFile`],
    /// [`LabelSet::EndOfVolume`] (continues on the next volume), or
    /// [`LabelSet::Header`] when the trailer is missing.
    pub trailer: LabelSet,
    /// Its index on this volume, from 0.
    pub index: u32,
}

/// A volume's labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeLabels {
    /// What the volume is.
    pub kind: VolumeKind,
    /// The files, for a labeled volume.
    pub files: Vec<FileInfo>,
}

/// Rewind and look at the first block, then rewind again.
pub fn identify<D: TapeTransport + ?Sized>(t: &mut D) -> Result<VolumeKind, TapeError> {
    t.rewind()?;
    t.set_block_size(0)?;
    let mut buf = vec![0u8; MAX_BLOCK];
    let kind = match t.read_block(&mut buf) {
        Ok(ReadOutcome::EndOfData) => VolumeKind::Blank,
        Ok(ReadOutcome::Block(LABEL_LEN)) => match Vol1::decode(&buf[..LABEL_LEN]) {
            Ok((vol1, code)) => VolumeKind::Labeled { vol1, code },
            Err(_) => VolumeKind::Unlabeled,
        },
        Ok(_) | Err(TransportError::BlockTooLarge { .. }) => VolumeKind::Unlabeled,
        Err(e) => return Err(e.into()),
    };
    t.rewind()?;
    Ok(kind)
}

/// One label block, or `None` at a tape mark or the end of data.
pub(super) fn read_label<D: TapeTransport + ?Sized>(
    t: &mut D,
    buf: &mut [u8],
) -> Result<Option<[u8; LABEL_LEN]>, TapeError> {
    match t.read_block(buf)? {
        ReadOutcome::Block(LABEL_LEN) => {
            Ok(Some(buf[..LABEL_LEN].try_into().expect("label length")))
        }
        ReadOutcome::Block(n) => Err(LabelError::Wrong {
            expected: "an 80-byte label",
            found: format!("a {n}-byte block"),
        }
        .into()),
        ReadOutcome::TapeMark | ReadOutcome::EndOfData => Ok(None),
    }
}

/// Read a label pair at the position.
pub(super) fn read_pair<D: TapeTransport + ?Sized>(
    t: &mut D,
    code: Code,
    buf: &mut [u8],
) -> Result<Option<(LabelSet, FileLabel)>, TapeError> {
    let Some(r1) = read_label(t, buf)? else {
        return Ok(None);
    };
    let r2 = read_label(t, buf)?.unwrap_or([0; LABEL_LEN]);
    Ok(Some(FileLabel::decode(&r1, &r2, code)?))
}

/// Expect a tape mark next.
pub(super) fn expect_mark<D: TapeTransport + ?Sized>(
    t: &mut D,
    buf: &mut [u8],
) -> Result<(), TapeError> {
    match t.read_block(buf)? {
        ReadOutcome::TapeMark => Ok(()),
        ReadOutcome::Block(n) => Err(LabelError::Wrong {
            expected: "a tape mark",
            found: format!("a {n}-byte block"),
        }
        .into()),
        ReadOutcome::EndOfData => Err(LabelError::Wrong {
            expected: "a tape mark",
            found: "the end of data".into(),
        }
        .into()),
    }
}

/// Every file on a labeled volume. Leaves the drive at the end of the
/// last file found.
pub(super) fn scan_files<D: TapeTransport + ?Sized>(
    t: &mut D,
    code: Code,
) -> Result<Vec<FileInfo>, TapeError> {
    let mut buf = vec![0u8; MAX_BLOCK];
    t.rewind()?;
    read_label(t, &mut buf)?;
    let mut files = Vec::new();
    loop {
        let index = u32::try_from(files.len()).expect("file count");
        let Some((set, mut label)) = read_pair(t, code, &mut buf)? else {
            break;
        };
        if set != LabelSet::Header {
            return Err(LabelError::Wrong {
                expected: "HDR1",
                found: format!("{set:?}"),
            }
            .into());
        }
        let mut trailer = LabelSet::Header;
        // Header mark, data and its mark, then the trailer pair.
        let found = expect_mark(t, &mut buf).and_then(|()| Ok(t.fsf(1)?));
        if found.is_ok()
            && let Some((set, tail)) = read_pair(t, code, &mut buf)?
        {
            trailer = set;
            label.block_count = tail.block_count;
            let _ = read_label(t, &mut buf)?;
        }
        files.push(FileInfo {
            label,
            trailer,
            index,
        });
        if trailer != LabelSet::EndOfFile {
            break;
        }
    }
    Ok(files)
}

/// Position at the HDR1 of the file at `index` on the volume.
pub(super) fn seek_header<D: TapeTransport + ?Sized>(
    t: &mut D,
    index: u32,
) -> Result<(), TapeError> {
    t.rewind()?;
    if index == 0 {
        t.fsr(1)?;
    } else {
        t.fsf(3 * index)?;
    }
    Ok(())
}

/// Position after the last complete file (where SEQNBR(*END) writes).
pub(super) fn seek_end<D: TapeTransport + ?Sized>(
    t: &mut D,
    files: &[FileInfo],
) -> Result<(), TapeError> {
    let complete = files
        .iter()
        .take_while(|f| f.trailer == LabelSet::EndOfFile)
        .count();
    seek_header(t, u32::try_from(complete).expect("file count"))
}

/// The number of files on an unlabeled volume.
pub(super) fn count_nl<D: TapeTransport + ?Sized>(t: &mut D) -> Result<u32, TapeError> {
    let mut buf = vec![0u8; MAX_BLOCK];
    t.rewind()?;
    let mut n = 0;
    loop {
        match t.read_block(&mut buf) {
            Ok(ReadOutcome::Block(_)) | Err(TransportError::BlockTooLarge { .. }) => {}
            Ok(ReadOutcome::TapeMark | ReadOutcome::EndOfData) => break,
            Err(e) => return Err(e.into()),
        }
        match t.fsf(1) {
            Ok(()) => n += 1,
            Err(TransportError::EndOfData) => {
                n += 1;
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(n)
}

/// DSPTAP DATA(*LABELS): what the volume is and every file on it.
pub fn labels<D: TapeTransport + ?Sized>(
    t: &mut D,
    vol: Option<&str>,
    end_opt: EndOpt,
) -> Result<VolumeLabels, TapeError> {
    let kind = identify(t)?;
    if let Some(want) = vol
        && kind.id() != want
    {
        return Err(TapeError::WrongVolume {
            expected: want.to_string(),
            found: kind.id(),
        });
    }
    let files = match &kind {
        VolumeKind::Labeled { code, .. } => scan_files(t, *code)?,
        _ => Vec::new(),
    };
    end_opt.apply(t)?;
    Ok(VolumeLabels { kind, files })
}
