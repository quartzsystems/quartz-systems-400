// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A save as one standard-labeled tape file: `VOL`, `SEQNBR`, `LABEL`,
//! `EXPDATE` and `ENDOPT` become the file's labels and position; the
//! stream goes in blocks of [`SAVE_BLOCK`] bytes (record format U), and
//! a file that reaches the early warning point continues on the next
//! volume through the provider.

use tape::label::{Date, Expiry, FileLabel, RecordFormat};
use tape::volume::{FileReader, FileWriter, ReadOptions, WriteOptions};
use tape::{EndOpt, MAX_BLOCK, SeqNbr, TapeTransport, VolumeProvider};

use crate::{MediaReader, MediaWriter, Result, SAVE_BLOCK, Written};

/// The tape parameters of a save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFile {
    /// `VOL`: the volumes in order; empty for `*MOUNTED`.
    pub vols: Vec<String>,
    /// `SEQNBR`.
    pub seq: SeqNbr,
    /// `LABEL`, resolved (`*LIB` is the library's name).
    pub label: String,
    /// `EXPDATE`.
    pub expires: Expiry,
    /// `ENDOPT`.
    pub end_opt: EndOpt,
    /// Today, for the labels and the active-file check.
    pub today: Date,
    /// `CLEAR(*ALL)`: active files may be overwritten.
    pub allow_active: bool,
    /// The job writing, for HDR2.
    pub job: String,
}

/// The tape parameters of a restore.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestoreFile {
    /// `VOL`: the volumes in order; empty for `*MOUNTED`.
    pub vols: Vec<String>,
    /// `SEQNBR`; `None` for `*SEARCH`.
    pub seq: Option<u32>,
    /// `LABEL`; `None` to take the file at `seq` whatever its label.
    pub label: Option<String>,
    /// `ENDOPT`.
    pub end_opt: EndOpt,
}

/// A label as the file identifier holds it: upper case, 17 characters.
#[must_use]
pub fn file_id(label: &str) -> String {
    label.trim().to_ascii_uppercase().chars().take(17).collect()
}

/// Writes a save to the drive.
pub struct TapeWriter<'a, D: TapeTransport + ?Sized> {
    drive: &'a mut D,
    provider: &'a mut dyn VolumeProvider<D>,
    file: SaveFile,
}

impl<'a, D: TapeTransport + ?Sized> TapeWriter<'a, D> {
    /// A writer for `file` on `drive`.
    pub fn new(drive: &'a mut D, provider: &'a mut dyn VolumeProvider<D>, file: SaveFile) -> Self {
        Self {
            drive,
            provider,
            file,
        }
    }
}

impl<D: TapeTransport + ?Sized> MediaWriter for TapeWriter<'_, D> {
    fn write_stream(&mut self, data: &[u8]) -> Result<Written> {
        let f = &self.file;
        let label = FileLabel {
            file_id: file_id(&f.label),
            created: Some(f.today),
            expires: f.expires,
            record_format: RecordFormat::Undefined,
            block_length: u32::try_from(SAVE_BLOCK).unwrap_or(u32::MAX),
            record_length: u32::try_from(SAVE_BLOCK).unwrap_or(u32::MAX),
            job_step: f.job.chars().take(17).collect(),
            ..FileLabel::default()
        };
        let opts = WriteOptions {
            vols: f.vols.clone(),
            seq: f.seq,
            label,
            unlabeled: false,
            today: Some(f.today),
            allow_active: f.allow_active,
            end_opt: f.end_opt,
        };
        let mut w = FileWriter::open(&mut *self.drive, &mut *self.provider, opts)?;
        for chunk in data.chunks(SAVE_BLOCK) {
            w.write_block(chunk)?;
        }
        let s = w.finish()?;
        Ok(Written {
            seq: s.file_seq,
            volumes: s.volumes,
            file: file_id(&self.file.label),
            bytes: data.len() as u64,
        })
    }
}

/// Reads a save from the drive.
pub struct TapeReader<'a, D: TapeTransport + ?Sized> {
    drive: &'a mut D,
    provider: &'a mut dyn VolumeProvider<D>,
    file: RestoreFile,
}

impl<'a, D: TapeTransport + ?Sized> TapeReader<'a, D> {
    /// A reader for `file` on `drive`.
    pub fn new(
        drive: &'a mut D,
        provider: &'a mut dyn VolumeProvider<D>,
        file: RestoreFile,
    ) -> Self {
        Self {
            drive,
            provider,
            file,
        }
    }
}

impl<D: TapeTransport + ?Sized> MediaReader for TapeReader<'_, D> {
    fn read_stream(&mut self) -> Result<(Vec<u8>, Vec<String>)> {
        let opts = ReadOptions {
            vols: self.file.vols.clone(),
            seq: self.file.seq,
            file_label: self.file.label.as_deref().map(file_id),
            unlabeled: false,
            end_opt: self.file.end_opt,
        };
        let mut r = FileReader::open(&mut *self.drive, &mut *self.provider, opts)?;
        let mut buf = vec![0u8; MAX_BLOCK];
        let mut out = Vec::new();
        while let Some(n) = r.read_block(&mut buf)? {
            out.extend_from_slice(&buf[..n]);
        }
        let vols = r.finish()?;
        Ok((out, vols))
    }
}
