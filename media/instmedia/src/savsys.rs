// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The install media layout on a tape and on an optical volume:
//! written by `SAVSYS` (and by `build media` for optical), read back by
//! the installer, recognised by `CHKTAP` and `DSPOPT DATA(*SAVRST)`.
//!
//! **Tape.** File 1 is the manifest (label [`MANIFEST_LABEL`]), then one
//! file per payload in manifest order, each labeled
//! [`Payload::label`], 64 KiB blocks, `*U` records. Files written
//! after them (a `SAVLIB *NONSYS`, a `SAV`) are the recovery's to
//! restore and are left alone here.
//!
//! **Optical.** `QS400/MEDIA.MANIFEST` and each payload at
//! [`Payload::path`].

use std::io::Read as _;

use ed25519_dalek::SigningKey;
use optical::manifest::{self, Manifest, ManifestFile};
use optical::{IsoImage, ReadAt, Volume};
use sha2::{Digest, Sha256};
use tape::label::{Date, Expiry, FileLabel, RecordFormat};
use tape::volume::{
    FileReader, FileWriter, NoMoreVolumes, ReadOptions, VolumeKind, WriteOptions, labels,
};
use tape::{EndOpt, MAX_BLOCK, SeqNbr, TapeTransport};

use crate::layout::{Contents, MANIFEST_LABEL, Payload};
use crate::{Error, Result};

/// A tape block.
const BLOCK: usize = 64 * 1024;

/// Media in memory: the manifest and the payload bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// The manifest, payload hashes filled in.
    pub manifest: Manifest,
    /// The payload, in manifest order.
    pub files: Vec<(Payload, Vec<u8>)>,
}

fn sha(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

impl Image {
    /// Media of `release` named `volume`: `savsys` for a system save,
    /// signed with `key` when given.
    #[must_use]
    pub fn new(
        release: release::Release,
        volume: &str,
        savsys: bool,
        files: Vec<(Payload, Vec<u8>)>,
        key: Option<&SigningKey>,
    ) -> Self {
        let mut manifest = Manifest {
            release: release.to_string(),
            volume: volume.to_string(),
            kind: if savsys {
                manifest::KIND_SAVSYS.into()
            } else {
                manifest::KIND_INSTALL.into()
            },
            files: files
                .iter()
                .map(|(p, d)| ManifestFile {
                    path: p.path(),
                    size: d.len() as u64,
                    sha256: sha(d),
                })
                .collect(),
            signature: None,
        };
        if let Some(k) = key {
            crate::sign::sign(&mut manifest, k);
        }
        Self { manifest, files }
    }

    /// One payload's bytes.
    #[must_use]
    pub fn get(&self, p: &Payload) -> Option<&[u8]> {
        self.files.iter().find(|(q, _)| q == p).map(|(_, d)| d.as_slice())
    }

    /// What the media holds.
    pub fn contents(&self) -> Result<Contents> {
        Contents::of(self.manifest.clone())
    }
}

/// Check one payload against its manifest line.
pub fn check(m: &Manifest, p: &Payload, data: &[u8]) -> Result<()> {
    let path = p.path();
    let f = m
        .file(&path)
        .ok_or_else(|| Error::media(format!("{path} is not in the media manifest")))?;
    if f.size != data.len() as u64 {
        return Err(Error::media(format!(
            "{path} is {} bytes, the manifest says {}",
            data.len(),
            f.size
        )));
    }
    if !f.sha256.eq_ignore_ascii_case(&sha(data)) {
        return Err(Error::media(format!("{path} does not match its hash in the manifest")));
    }
    Ok(())
}

/// Something the installer reads install media from.
pub trait Source {
    /// What it holds.
    fn contents(&self) -> &Contents;
    /// One payload, checked against the manifest.
    fn read(&mut self, p: &Payload) -> Result<Vec<u8>>;
}

impl Source for (Image, Contents) {
    fn contents(&self) -> &Contents {
        &self.1
    }
    fn read(&mut self, p: &Payload) -> Result<Vec<u8>> {
        let d = self
            .0
            .get(p)
            .ok_or_else(|| Error::media(format!("{} is not on the media", p.path())))?
            .to_vec();
        check(&self.0.manifest, p, &d)?;
        Ok(d)
    }
}

// ---------------------------------------------------------------- tape

fn file_label(id: &str, today: Date) -> FileLabel {
    FileLabel {
        file_id: id.into(),
        created: Some(today),
        expires: Expiry::None,
        record_format: RecordFormat::Undefined,
        block_length: u32::try_from(BLOCK).expect("small"),
        record_length: u32::try_from(BLOCK).expect("small"),
        system_code: "QS400".into(),
        ..FileLabel::default()
    }
}

fn write_file<D: TapeTransport + ?Sized>(
    t: &mut D,
    seq: u32,
    id: &str,
    data: &[u8],
    today: Date,
) -> Result<()> {
    let opts = WriteOptions {
        vols: Vec::new(),
        seq: SeqNbr::Number(seq),
        label: file_label(id, today),
        unlabeled: false,
        today: Some(today),
        allow_active: false,
        end_opt: EndOpt::Leave,
    };
    let mut more = NoMoreVolumes;
    let mut w = FileWriter::open(t, &mut more, opts)?;
    for chunk in data.chunks(BLOCK) {
        w.write_block(chunk)?;
    }
    w.finish()?;
    Ok(())
}

/// Write `img` onto a labeled tape from file 1 on: what `SAVSYS` does.
/// Files after them are lost, as a save at sequence 1 loses them.
pub fn write_tape<D: TapeTransport + ?Sized>(t: &mut D, img: &Image, today: Date) -> Result<u32> {
    let kind = tape::volume::identify(t)?;
    if !matches!(kind, VolumeKind::Labeled { .. }) {
        return Err(Error::media(format!(
            "the volume is {}; initialize it with INZTAP first",
            kind.id()
        )));
    }
    write_file(t, 1, MANIFEST_LABEL, img.manifest.to_text().as_bytes(), today)?;
    let mut seq = 1;
    for (p, d) in &img.files {
        seq += 1;
        write_file(t, seq, &p.label(), d, today)?;
    }
    t.rewind().map_err(tape::TapeError::from)?;
    Ok(seq)
}

fn read_file<D: TapeTransport + ?Sized>(t: &mut D, seq: u32, id: &str) -> Result<Vec<u8>> {
    let opts = ReadOptions {
        seq: Some(seq),
        file_label: Some(id.to_string()),
        end_opt: EndOpt::Leave,
        ..ReadOptions::default()
    };
    let mut more = NoMoreVolumes;
    let mut r = FileReader::open(t, &mut more, opts)?;
    let mut buf = vec![0u8; MAX_BLOCK];
    let mut out = Vec::new();
    while let Some(n) = r.read_block(&mut buf)? {
        out.extend_from_slice(&buf[..n]);
    }
    r.finish()?;
    Ok(out)
}

/// The manifest of install media on a tape, or `None` when the tape is
/// something else (`CHKTAP` and the installer's device search).
pub fn recognize_tape<D: TapeTransport + ?Sized>(t: &mut D) -> Result<Option<Contents>> {
    let l = labels(t, None, EndOpt::Rewind)?;
    let Some(first) = l.files.first() else {
        return Ok(None);
    };
    if first.label.file_id.trim() != MANIFEST_LABEL {
        return Ok(None);
    }
    let text = read_file(t, first.label.file_seq, MANIFEST_LABEL)?;
    t.rewind().map_err(tape::TapeError::from)?;
    let m = Manifest::parse(&String::from_utf8_lossy(&text))?;
    Contents::of(m).map(Some)
}

/// Install media on a tape: the manifest read once, payloads on demand.
pub struct TapeSource<'a, D: TapeTransport + ?Sized> {
    t: &'a mut D,
    contents: Contents,
    seqs: Vec<(Payload, u32)>,
}

impl<'a, D: TapeTransport + ?Sized> TapeSource<'a, D> {
    /// Open the media on `t`.
    pub fn open(t: &'a mut D) -> Result<Self> {
        let contents =
            recognize_tape(t)?.ok_or_else(|| Error::media("the tape is not install media"))?;
        let l = labels(t, None, EndOpt::Rewind)?;
        let seqs = l
            .files
            .iter()
            .filter_map(|f| Payload::from_label(&f.label.file_id).map(|p| (p, f.label.file_seq)))
            .collect();
        Ok(Self { t, contents, seqs })
    }
}

impl<D: TapeTransport + ?Sized> Source for TapeSource<'_, D> {
    fn contents(&self) -> &Contents {
        &self.contents
    }

    fn read(&mut self, p: &Payload) -> Result<Vec<u8>> {
        let seq = self
            .seqs
            .iter()
            .find(|(q, _)| q == p)
            .map(|(_, s)| *s)
            .ok_or_else(|| Error::media(format!("{} is not on the tape", p.label())))?;
        let d = read_file(self.t, seq, &p.label())?;
        self.t.rewind().map_err(tape::TapeError::from)?;
        check(&self.contents.manifest, p, &d)?;
        Ok(d)
    }
}

/// Read all of a tape's install media.
pub fn read_tape<D: TapeTransport + ?Sized>(t: &mut D) -> Result<Image> {
    let mut s = TapeSource::open(t)?;
    let payloads = s.contents.payloads.clone();
    let manifest = s.contents.manifest.clone();
    let mut files = Vec::new();
    for p in payloads {
        let d = s.read(&p)?;
        files.push((p, d));
    }
    Ok(Image { manifest, files })
}

// ------------------------------------------------------------- optical

/// Write `img` onto a writable optical volume, replacing what is there
/// under the same names, and flush.
pub fn write_optical(v: &mut Volume, img: &Image, epoch: u64) -> Result<()> {
    for dir in ["QS400", crate::layout::LP_DIR, crate::layout::SAVSYS_DIR] {
        if v.stat(dir).is_err() {
            v.mkdir(dir)?;
        }
    }
    v.put_file(manifest::PATH, img.manifest.to_text().into_bytes(), true)?;
    for (p, d) in &img.files {
        v.put_file(&p.path(), d.clone(), true)?;
    }
    v.flush(epoch)?;
    Ok(())
}

/// The install media on an ISO 9660 image, or `None` (`DSPOPT
/// DATA(*SAVRST)` and the installer's device search).
pub fn recognize_iso<R: ReadAt>(iso: &IsoImage<R>) -> Result<Option<Contents>> {
    let Ok(e) = iso.lookup(manifest::PATH) else {
        return Ok(None);
    };
    let text = iso.read_file(&e)?;
    let m = Manifest::parse(&String::from_utf8_lossy(&text))?;
    Contents::of(m).map(Some)
}

/// Install media on an ISO 9660 image.
pub struct IsoSource<R: ReadAt> {
    iso: IsoImage<R>,
    contents: Contents,
}

impl<R: ReadAt> IsoSource<R> {
    /// Open the media on `iso`.
    pub fn open(iso: IsoImage<R>) -> Result<Self> {
        let contents =
            recognize_iso(&iso)?.ok_or_else(|| Error::media("the volume is not install media"))?;
        Ok(Self { iso, contents })
    }
}

impl<R: ReadAt> Source for IsoSource<R> {
    fn contents(&self) -> &Contents {
        &self.contents
    }

    fn read(&mut self, p: &Payload) -> Result<Vec<u8>> {
        let e = self.iso.lookup(&p.path())?;
        let d = self.iso.read_file(&e)?;
        check(&self.contents.manifest, p, &d)?;
        Ok(d)
    }
}

/// Install media in a directory tree (a mounted CD or a FAT copy).
pub struct DirSource {
    root: std::path::PathBuf,
    contents: Contents,
}

impl DirSource {
    /// Open the media mounted at `root`.
    pub fn open(root: &std::path::Path) -> Result<Self> {
        let text = std::fs::read_to_string(root.join(manifest::PATH))
            .map_err(|e| Error::media(format!("{}: {e}", manifest::PATH)))?;
        let contents = Contents::of(Manifest::parse(&text)?)?;
        Ok(Self {
            root: root.to_path_buf(),
            contents,
        })
    }
}

impl Source for DirSource {
    fn contents(&self) -> &Contents {
        &self.contents
    }

    fn read(&mut self, p: &Payload) -> Result<Vec<u8>> {
        let path = p.path();
        let mut f = std::fs::File::open(self.root.join(&path))
            .map_err(|e| Error::media(format!("{path} on the media: {e}")))?;
        let mut d = Vec::new();
        f.read_to_end(&mut d)
            .map_err(|e| Error::media(format!("{path} on the media: {e}")))?;
        check(&self.contents.manifest, p, &d)?;
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::Arc;

    use optical::MemImage;
    use tape::volume::{Check, InzOptions, inztap};
    use tape::{VirtualTape, VolumeOptions};

    use super::*;

    fn image(savsys: bool) -> Image {
        let key = SigningKey::from_bytes(&[3; 32]);
        Image::new(
            release::RELEASE,
            "SAVSYS01",
            savsys,
            vec![
                (Payload::LicA, vec![1; 300_000]),
                (Payload::LicB, vec![2; 10]),
                (Payload::Product("QS00SS1".into()), vec![3; 600_000]),
                (Payload::SecData, vec![4; 5]),
                (Payload::Config, Vec::new()),
            ],
            Some(&key),
        )
    }

    fn today() -> Date {
        Date::from_ymd(2026, 10, 31).unwrap()
    }

    #[test]
    fn a_tape_round_trips_and_is_recognised() {
        let opts = VolumeOptions::imgsiz(8, "SAVSYS");
        let mut t = VirtualTape::create(Cursor::new(Vec::new()), &opts).unwrap();
        let img = image(true);
        // Unlabeled volumes are refused.
        assert!(write_tape(&mut t, &img, today()).is_err());
        assert!(recognize_tape(&mut t).unwrap_or(None).is_none());
        inztap(
            &mut t,
            &InzOptions {
                new_vol: Some("SAV001".into()),
                new_owner: "QS400".into(),
                check: Check::No,
                ..InzOptions::default()
            },
        )
        .unwrap();
        assert!(recognize_tape(&mut t).unwrap().is_none());
        assert_eq!(write_tape(&mut t, &img, today()).unwrap(), 6);
        let c = recognize_tape(&mut t).unwrap().unwrap();
        assert!(c.savsys && c.has_os());
        assert_eq!(c.release, release::RELEASE);
        let back = read_tape(&mut t).unwrap();
        assert_eq!(back, img);
        let public = SigningKey::from_bytes(&[3; 32]).verifying_key().to_bytes();
        crate::sign::verify(&back.manifest, &public).unwrap();
        // One payload on demand.
        let mut s = TapeSource::open(&mut t).unwrap();
        assert_eq!(s.read(&Payload::SecData).unwrap(), vec![4; 5]);
        assert!(s.read(&Payload::Product("QS00WDS".into())).is_err());
    }

    #[test]
    fn an_optical_volume_round_trips_and_is_recognised() {
        let store = Arc::new(MemImage::default());
        let mut v = Volume::create(store.clone(), "SAVSYS01", "", 64 << 20, 1).unwrap();
        let img = image(true);
        write_optical(&mut v, &img, 2).unwrap();
        let iso = IsoImage::open(store.clone()).unwrap();
        let c = recognize_iso(&iso).unwrap().unwrap();
        assert!(c.savsys);
        assert_eq!(c.products(), vec!["QS00SS1".to_string()]);
        let mut s = IsoSource::open(iso).unwrap();
        assert_eq!(s.read(&Payload::LicB).unwrap(), vec![2; 10]);
        assert_eq!(s.read(&Payload::Config).unwrap(), Vec::<u8>::new());
        // A plain volume is not install media.
        let other = Arc::new(MemImage::default());
        Volume::create(other.clone(), "DATA01", "", 1 << 20, 1).unwrap();
        assert!(recognize_iso(&IsoImage::open(other).unwrap()).unwrap().is_none());
    }

    #[test]
    fn a_changed_payload_is_refused() {
        let img = image(false);
        let c = img.contents().unwrap();
        assert!(!c.savsys);
        let mut bad = img.clone();
        bad.files[1].1 = vec![9; 10];
        let mut s = (bad, c);
        assert!(s.read(&Payload::LicB).unwrap_err().to_string().contains("hash"));
        assert!(s.read(&Payload::LicA).is_ok());
    }
}
