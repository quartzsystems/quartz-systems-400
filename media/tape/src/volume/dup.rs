// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DUPTAP: copy files from one volume to another, labels and all. The
//! target keeps its own VOL1.

use super::read::{FileReader, ReadOptions};
use super::scan::{VolumeKind, count_nl, identify, scan_files};
use super::write::{FileWriter, WriteOptions, WriteSummary};
use super::{EndOpt, SeqNbr, VolumeProvider};
use crate::TapeError;
use crate::label::{Date, FileLabel};
use crate::transport::{MAX_BLOCK, TapeTransport};

/// FROMSEQNBR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DupSeq {
    /// FROMSEQNBR(*ALL).
    #[default]
    All,
    /// FROMSEQNBR(start end), inclusive.
    Range(u32, u32),
}

/// TOSEQNBR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToSeq {
    /// TOSEQNBR(*FROMSEQ): the same sequence numbers as the source.
    #[default]
    FromSeq,
    /// TOSEQNBR(*END): after the target's last file.
    End,
    /// TOSEQNBR(n): the first file at n, the next at n + 1 and so on.
    Number(u32),
}

/// DUPTAP's parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DupOptions {
    /// FROMVOL: `None` for *MOUNTED.
    pub from_vol: Option<String>,
    /// TOVOL: `None` for *MOUNTED.
    pub to_vol: Option<String>,
    /// FROMSEQNBR.
    pub from_seq: DupSeq,
    /// TOSEQNBR.
    pub to_seq: ToSeq,
    /// Today, for the active-file check on the target.
    pub today: Option<Date>,
    /// Overwrite active files on the target.
    pub allow_active: bool,
    /// FROMENDOPT.
    pub from_end_opt: EndOpt,
    /// TOENDOPT.
    pub to_end_opt: EndOpt,
}

/// Duplicate files from the volume on `from` to the volume on `to`.
#[allow(clippy::too_many_lines)]
pub fn duptap<F, T>(
    from: &mut F,
    from_provider: &mut dyn VolumeProvider<F>,
    to: &mut T,
    to_provider: &mut dyn VolumeProvider<T>,
    opts: &DupOptions,
) -> Result<Vec<WriteSummary>, TapeError>
where
    F: TapeTransport + ?Sized,
    T: TapeTransport + ?Sized,
{
    let kind = identify(from)?;
    if let Some(want) = &opts.from_vol
        && &kind.id() != want
    {
        return Err(TapeError::WrongVolume {
            expected: want.clone(),
            found: kind.id(),
        });
    }
    let unlabeled = !matches!(kind, VolumeKind::Labeled { .. });
    let seqs: Vec<u32> = match &kind {
        VolumeKind::Labeled { code, .. } => scan_files(from, *code)?
            .into_iter()
            .map(|f| f.label.file_seq)
            .collect(),
        VolumeKind::Unlabeled => (1..=count_nl(from)?).collect(),
        VolumeKind::Blank => Vec::new(),
    };
    let chosen: Vec<u32> = seqs
        .into_iter()
        .filter(|&s| match opts.from_seq {
            DupSeq::All => true,
            DupSeq::Range(a, b) => (a..=b).contains(&s),
        })
        .collect();
    if chosen.is_empty() {
        return Err(TapeError::FileNotFound {
            what: match opts.from_seq {
                DupSeq::All => "*ALL".into(),
                DupSeq::Range(a, b) => format!("{a} to {b}"),
            },
        });
    }
    let mut out = Vec::new();
    let mut buf = vec![0u8; MAX_BLOCK];
    for (i, &seq) in chosen.iter().enumerate() {
        let mut reader = FileReader::open(
            from,
            from_provider,
            ReadOptions {
                vols: opts.from_vol.iter().cloned().collect(),
                seq: Some(seq),
                file_label: None,
                unlabeled,
                end_opt: EndOpt::Leave,
            },
        )?;
        let target = match opts.to_seq {
            ToSeq::FromSeq => SeqNbr::Number(seq),
            ToSeq::End => SeqNbr::End,
            ToSeq::Number(n) => SeqNbr::Number(n + u32::try_from(i).expect("file count")),
        };
        let label = FileLabel {
            continued: false,
            volume_seq: 1,
            ..reader.label().clone()
        };
        let mut writer = FileWriter::open(
            to,
            to_provider,
            WriteOptions {
                vols: opts.to_vol.iter().cloned().collect(),
                seq: target,
                label,
                unlabeled,
                today: opts.today,
                allow_active: opts.allow_active,
                end_opt: EndOpt::Leave,
            },
        )?;
        while let Some(n) = reader.read_block(&mut buf)? {
            writer.write_block(&buf[..n])?;
        }
        out.push(writer.finish()?);
        reader.finish()?;
    }
    opts.from_end_opt.apply(from)?;
    opts.to_end_opt.apply(to)?;
    Ok(out)
}
