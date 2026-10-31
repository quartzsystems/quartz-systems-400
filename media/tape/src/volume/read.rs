// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading one file's data, across volumes where it continues.

use super::scan::{VolumeKind, expect_mark, identify, read_pair, scan_files, seek_header};
use super::{EndOpt, VolumeProvider, VolumeRequest};
use crate::TapeError;
use crate::label::{Code, FileLabel, LabelSet};
use crate::transport::{MAX_BLOCK, ReadOutcome, TapeTransport};

/// Which file to read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadOptions {
    /// VOL: the volumes in order; empty for VOL(*MOUNTED).
    pub vols: Vec<String>,
    /// SEQNBR, or `None` to find the file by label alone (or take the
    /// first file when there is no label either).
    pub seq: Option<u32>,
    /// FILELABEL, checked when given.
    pub file_label: Option<String>,
    /// The volume is unlabeled; `seq` counts its files.
    pub unlabeled: bool,
    /// ENDOPT.
    pub end_opt: EndOpt,
}

/// A file being read.
pub struct FileReader<'a, D: TapeTransport + ?Sized> {
    t: &'a mut D,
    provider: &'a mut dyn VolumeProvider<D>,
    opts: ReadOptions,
    code: Code,
    label: FileLabel,
    volumes: Vec<String>,
    labels: Vec<u8>,
    done: bool,
}

impl<'a, D: TapeTransport + ?Sized> FileReader<'a, D> {
    /// Find the file and position at its first data block.
    pub fn open(
        t: &'a mut D,
        provider: &'a mut dyn VolumeProvider<D>,
        opts: ReadOptions,
    ) -> Result<Self, TapeError> {
        let kind = identify(t)?;
        if let Some(w) = opts.vols.first()
            && &kind.id() != w
        {
            return Err(TapeError::WrongVolume {
                expected: w.clone(),
                found: kind.id(),
            });
        }
        let mut r = Self {
            t,
            provider,
            code: Code::Ebcdic,
            label: FileLabel::default(),
            volumes: vec![kind.id()],
            labels: vec![0u8; MAX_BLOCK],
            done: false,
            opts,
        };
        match kind {
            VolumeKind::Labeled { code, .. } if !r.opts.unlabeled => {
                r.code = code;
                let files = scan_files(r.t, code)?;
                let found = files.iter().find(|f| {
                    r.opts.seq.is_none_or(|s| f.label.file_seq == s)
                        && r.opts.file_label.as_ref().is_none_or(|l| &f.label.file_id == l)
                });
                let Some(f) = found else {
                    return Err(TapeError::FileNotFound {
                        what: r.wanted(),
                    });
                };
                seek_header(r.t, f.index)?;
                let (_, label) = read_pair(r.t, code, &mut r.labels)?.expect("scanned header");
                expect_mark(r.t, &mut r.labels)?;
                r.label = label;
            }
            VolumeKind::Unlabeled if r.opts.unlabeled => {
                let seq = r.opts.seq.unwrap_or(1).max(1);
                r.t.rewind()?;
                if seq > 1 {
                    r.t.fsf(seq - 1).map_err(|_| TapeError::FileNotFound {
                        what: seq.to_string(),
                    })?;
                }
                r.label.file_seq = seq;
            }
            VolumeKind::Labeled { .. } => return Err(TapeError::Labeled),
            _ => return Err(TapeError::NotLabeled),
        }
        Ok(r)
    }

    fn wanted(&self) -> String {
        match (&self.opts.seq, &self.opts.file_label) {
            (Some(s), Some(l)) => format!("{s} ({l})"),
            (Some(s), None) => s.to_string(),
            (None, Some(l)) => l.clone(),
            (None, None) => "1".into(),
        }
    }

    /// The file's header labels (on the volume being read).
    #[must_use]
    pub fn label(&self) -> &FileLabel {
        &self.label
    }

    /// The volumes read so far.
    #[must_use]
    pub fn volumes(&self) -> &[String] {
        &self.volumes
    }

    /// Read the next data block; `None` at the end of the file.
    pub fn read_block(&mut self, buf: &mut [u8]) -> Result<Option<usize>, TapeError> {
        loop {
            if self.done {
                return Ok(None);
            }
            match self.t.read_block(buf)? {
                ReadOutcome::Block(n) => return Ok(Some(n)),
                ReadOutcome::EndOfData => {
                    self.done = true;
                }
                ReadOutcome::TapeMark if self.opts.unlabeled => self.done = true,
                ReadOutcome::TapeMark => {
                    match read_pair(self.t, self.code, &mut self.labels)? {
                        Some((LabelSet::EndOfVolume, _)) => self.continue_on_next()?,
                        _ => self.done = true,
                    }
                }
            }
        }
    }

    fn continue_on_next(&mut self) -> Result<(), TapeError> {
        let n = self.volumes.len();
        let req = VolumeRequest {
            volume_seq: self.label.volume_seq + 1,
            expected: self.opts.vols.get(n).cloned(),
            previous: self.volumes[n - 1].clone(),
            file_label: self.label.file_id.clone(),
            file_seq: self.label.file_seq,
            writing: false,
        };
        self.provider.next_volume(self.t, &req)?;
        let kind = identify(self.t)?;
        if let Some(w) = &req.expected
            && &kind.id() != w
        {
            return Err(TapeError::WrongVolume {
                expected: w.clone(),
                found: kind.id(),
            });
        }
        let VolumeKind::Labeled { vol1, code } = kind else {
            return Err(TapeError::NotLabeled);
        };
        self.code = code;
        seek_header(self.t, 0)?;
        let found = read_pair(self.t, code, &mut self.labels)?;
        let ok = matches!(&found, Some((LabelSet::Header, l))
            if l.file_id == self.label.file_id
                && l.file_seq == self.label.file_seq
                && l.volume_seq == req.volume_seq);
        if !ok {
            return Err(TapeError::NotContinuation {
                volume: vol1.volume_id,
                file: self.label.file_id.clone(),
                found: found.map_or_else(|| "no file".into(), |(_, l)| l.file_id),
            });
        }
        expect_mark(self.t, &mut self.labels)?;
        self.label = found.expect("checked").1;
        self.volumes.push(vol1.volume_id);
        Ok(())
    }

    /// Stop reading and apply ENDOPT.
    pub fn finish(self) -> Result<Vec<String>, TapeError> {
        self.opts.end_opt.apply(self.t)?;
        Ok(self.volumes)
    }
}
