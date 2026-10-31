// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing one file: labeled at SEQNBR n or *END, continued across
//! volumes at the early warning point; or unlabeled.

use super::inz::refuse_active;
use super::scan::{FileInfo, VolumeKind, count_nl, identify, scan_files, seek_end, seek_header};
use super::{EndOpt, SeqNbr, VolumeProvider, VolumeRequest};
use crate::TapeError;
use crate::label::{Code, Date, FileLabel, LabelSet, SYSTEM_CODE};
use crate::transport::{TapeTransport, TransportError};

/// How to write a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOptions {
    /// VOL: the volumes in order; empty for VOL(*MOUNTED).
    pub vols: Vec<String>,
    /// SEQNBR.
    pub seq: SeqNbr,
    /// The labels to write: file identifier, dates, record format and
    /// lengths. Sequence numbers, set identifier and block counts are
    /// filled in.
    pub label: FileLabel,
    /// Write without labels (the volume must be unlabeled).
    pub unlabeled: bool,
    /// Today, for the active-file check; `None` skips it.
    pub today: Option<Date>,
    /// Overwrite active files (the operator answered the inquiry).
    pub allow_active: bool,
    /// ENDOPT.
    pub end_opt: EndOpt,
}

/// What a finished write produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteSummary {
    /// The file sequence number written.
    pub file_seq: u32,
    /// The volumes it is on, in order.
    pub volumes: Vec<String>,
    /// Data blocks in all.
    pub blocks: u64,
}

/// A file being written.
pub struct FileWriter<'a, D: TapeTransport + ?Sized> {
    t: &'a mut D,
    provider: &'a mut dyn VolumeProvider<D>,
    opts: WriteOptions,
    code: Code,
    label: FileLabel,
    volumes: Vec<String>,
    blocks_here: u64,
    blocks: u64,
}

fn mounted_check(kind: &VolumeKind, want: Option<&String>) -> Result<(), TapeError> {
    match want {
        Some(w) if &kind.id() != w => Err(TapeError::WrongVolume {
            expected: w.clone(),
            found: kind.id(),
        }),
        _ => Ok(()),
    }
}

impl<'a, D: TapeTransport + ?Sized> FileWriter<'a, D> {
    /// Position and write the header labels.
    pub fn open(
        t: &'a mut D,
        provider: &'a mut dyn VolumeProvider<D>,
        opts: WriteOptions,
    ) -> Result<Self, TapeError> {
        if t.status()?.write_protected {
            return Err(TransportError::WriteProtected.into());
        }
        let kind = identify(t)?;
        mounted_check(&kind, opts.vols.first())?;
        let mut w = Self {
            t,
            provider,
            code: Code::Ebcdic,
            label: opts.label.clone(),
            volumes: vec![kind.id()],
            blocks_here: 0,
            blocks: 0,
            opts,
        };
        match kind {
            VolumeKind::Labeled { vol1, code } => {
                w.code = code;
                if w.opts.unlabeled {
                    return Err(TapeError::Labeled);
                }
                let files = scan_files(w.t, code)?;
                let seq = w.place(&vol1.volume_id, &files)?;
                w.label.file_seq = seq;
                w.label.set_id = vol1.volume_id;
                w.label.volume_seq = 1;
                w.label.continued = false;
                w.headers()?;
            }
            VolumeKind::Unlabeled | VolumeKind::Blank if w.opts.unlabeled => {
                let have = if kind == VolumeKind::Blank { 0 } else { count_nl(w.t)? };
                let seq = match w.opts.seq {
                    SeqNbr::End => have + 1,
                    SeqNbr::Number(n) if n >= 1 && n <= have + 1 => n,
                    SeqNbr::Number(n) => {
                        return Err(TapeError::FileNotFound {
                            what: n.to_string(),
                        });
                    }
                };
                w.t.rewind()?;
                if seq > 1 {
                    w.t.fsf(seq - 1)?;
                }
                w.label.file_seq = seq;
            }
            _ => return Err(TapeError::NotLabeled),
        }
        Ok(w)
    }

    /// Check SEQNBR against the files and position at the HDR1 to write.
    fn place(&mut self, volume: &str, files: &[FileInfo]) -> Result<u32, TapeError> {
        let first = files.first().map_or(1, |f| f.label.file_seq);
        let next = files
            .iter()
            .take_while(|f| f.trailer == LabelSet::EndOfFile)
            .last()
            .map_or(first, |f| f.label.file_seq + 1);
        let seq = match self.opts.seq {
            SeqNbr::End => next,
            SeqNbr::Number(n) if n == next || files.iter().any(|f| f.label.file_seq == n) => n,
            SeqNbr::Number(n) => {
                return Err(TapeError::FileNotFound {
                    what: n.to_string(),
                });
            }
        };
        let doomed: Vec<FileInfo> = files
            .iter()
            .filter(|f| f.label.file_seq >= seq)
            .cloned()
            .collect();
        if !self.opts.allow_active {
            refuse_active(volume, &doomed, false, self.opts.today)?;
        }
        match files.iter().find(|f| f.label.file_seq == seq) {
            Some(f) => seek_header(self.t, f.index)?,
            None => seek_end(self.t, files)?,
        }
        Ok(seq)
    }

    fn headers(&mut self) -> Result<(), TapeError> {
        self.t.set_block_size(0)?;
        if self.label.system_code.is_empty() {
            self.label.system_code = SYSTEM_CODE.to_string();
        }
        let (h1, h2) = self.label.encode(LabelSet::Header, self.code)?;
        self.t.write_block(&h1)?;
        self.t.write_block(&h2)?;
        self.t.write_eof(1)?;
        Ok(())
    }

    fn trailers(&mut self, set: LabelSet) -> Result<(), TapeError> {
        let mut l = self.label.clone();
        l.block_count = self.blocks_here;
        let (r1, r2) = l.encode(set, self.code)?;
        self.t.write_eof(1)?;
        self.t.write_block(&r1)?;
        self.t.write_block(&r2)?;
        self.t.write_eof(2)?;
        Ok(())
    }

    /// The label being written (sequence number filled in).
    #[must_use]
    pub fn label(&self) -> &FileLabel {
        &self.label
    }

    /// Write one data block.
    pub fn write_block(&mut self, data: &[u8]) -> Result<(), TapeError> {
        match self.t.write_block(data) {
            Ok(w) => {
                self.count();
                if w.early_warning && !self.opts.unlabeled {
                    self.switch()?;
                }
                Ok(())
            }
            Err(TransportError::EndOfMedium) if !self.opts.unlabeled => {
                self.switch()?;
                self.t.write_block(data)?;
                self.count();
                Ok(())
            }
            Err(TransportError::EndOfMedium) => Err(TapeError::EndOfVolume),
            Err(e) => Err(e.into()),
        }
    }

    fn count(&mut self) {
        self.blocks_here += 1;
        self.blocks += 1;
    }

    /// End this volume with EOV labels and continue on the next.
    fn switch(&mut self) -> Result<(), TapeError> {
        self.trailers(LabelSet::EndOfVolume)
            .map_err(|e| match e {
                TapeError::Transport(TransportError::EndOfMedium) => TapeError::EndOfVolume,
                e => e,
            })?;
        let n = self.volumes.len();
        let req = VolumeRequest {
            volume_seq: u32::try_from(n + 1).expect("volume count"),
            expected: self.opts.vols.get(n).cloned(),
            previous: self.volumes[n - 1].clone(),
            file_label: self.label.file_id.clone(),
            file_seq: self.label.file_seq,
            writing: true,
        };
        self.provider.next_volume(self.t, &req)?;
        if self.t.status()?.write_protected {
            return Err(TransportError::WriteProtected.into());
        }
        let kind = identify(self.t)?;
        mounted_check(&kind, req.expected.as_ref())?;
        let VolumeKind::Labeled { vol1, code } = kind else {
            return Err(TapeError::NotLabeled);
        };
        if !self.opts.allow_active {
            let files = scan_files(self.t, code)?;
            refuse_active(&vol1.volume_id, &files, false, self.opts.today)?;
        }
        self.code = code;
        self.volumes.push(vol1.volume_id);
        seek_header(self.t, 0)?;
        self.label.volume_seq = req.volume_seq;
        self.label.continued = true;
        self.blocks_here = 0;
        self.headers()
    }

    /// Write the trailer labels and apply ENDOPT.
    pub fn finish(mut self) -> Result<WriteSummary, TapeError> {
        if self.opts.unlabeled {
            self.t.write_eof(2)?;
        } else {
            self.trailers(LabelSet::EndOfFile)?;
        }
        self.t.bsf(1)?;
        self.opts.end_opt.apply(self.t)?;
        Ok(WriteSummary {
            file_seq: self.label.file_seq,
            volumes: self.volumes,
            blocks: self.blocks,
        })
    }
}
