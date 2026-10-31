// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A PTF as a save stream: built from objects in a store (the build,
//! `QSCRTPTF`) and opened with every signature checked (`LODPTF`).

use std::collections::BTreeMap;
use std::io::Cursor;

use ed25519_dalek::SigningKey;
use savfmt::object::flags;
use savfmt::{Compression, MediaHeader, ObjectDescriptor, Part, Reader, SaveKind, Scan, Writer};
use saverst::sign;
use saverst::{AllowAll, ObjSel, SaveOptions};
use store::Store;
use store::obj::{JobHandle, ObjectId, Signature, ops};

use crate::header::PtfHeader;
use crate::{Error, HEADER_TYPE, LIC_TYPE, Result};

/// A replacement object: library, name, type code.
pub type ObjRef = (String, String, u16);

/// What `QSCRTPTF` (or the build) packages.
pub struct Build<'a> {
    /// The header; `object_count` is filled in.
    pub header: PtfHeader,
    /// The cover letter.
    pub cover: String,
    /// The objects, in the store given.
    pub objects: Vec<ObjRef>,
    /// A new UKI, for a LIC PTF.
    pub lic: Option<&'a [u8]>,
    /// The slot B build of the same UKI; slot A's serves both when absent.
    pub lic_b: Option<&'a [u8]>,
    /// The signing key.
    pub key: &'a SigningKey,
    /// The system name for the media header.
    pub system: String,
}

fn media_header(ptf: &str, system: &str, now: u64) -> MediaHeader {
    let r = release::RELEASE;
    let mut id = [0u8; 16];
    id[..8].copy_from_slice(ptf.as_bytes().get(..8).unwrap_or(b"QS000000"));
    id[8..].copy_from_slice(&now.to_be_bytes());
    MediaHeader {
        release: (r.v, r.r, r.m),
        target: (r.v, r.r, r.m),
        compression: Compression::Medium,
        kind: SaveKind::Ptf,
        created: now,
        stream_id: id,
        system: system.into(),
        serial: String::new(),
        command: "QSCRTPTF".into(),
        user: "QSYS".into(),
    }
}

/// A pseudo-object (the header, the LIC image): written and signed.
fn pseudo(
    w: &mut Writer<Vec<u8>>,
    key: &SigningKey,
    type_code: u16,
    name: &str,
    parts: &BTreeMap<Part, Vec<u8>>,
) -> Result<()> {
    let d = ObjectDescriptor {
        type_code,
        flags: flags::SIGNED,
        name: savfmt::name10(name),
        size_bytes: parts.values().map(|p| p.len() as u64).sum(),
        part_count: u16::try_from(parts.len()).expect("parts"),
        ..ObjectDescriptor::default()
    };
    w.begin_object(d, &[])?;
    for (p, b) in parts {
        w.part(*p, b)?;
    }
    let digest = w.end_object()?;
    let public = key.verifying_key().to_bytes();
    w.signature(sign::key_id(&public), sign::sign(key, &digest))?;
    Ok(())
}

/// Sign `id` in place: its header carries the signature it is saved with.
fn sign_object(store: &mut Store, id: ObjectId, key: &SigningKey) -> Result<()> {
    let mut t = store.begin()?;
    let digest = sign::content_digest(&mut t, id)?;
    let (page, mut h) = ops::read_header(&mut t, id)?;
    let public = key.verifying_key().to_bytes();
    h.signature = Some(Signature {
        bytes: sign::sign(key, &digest),
        key_id: sign::key_id(&public),
        system: true,
    });
    ops::write_header(&mut t, page, &h)?;
    t.commit()?;
    Ok(())
}

/// Package a PTF from objects in `store`. Every object is signed.
pub fn build(store: &mut Store, qsys: ObjectId, b: &Build<'_>, now: u64) -> Result<Vec<u8>> {
    // Sign the objects where they are.
    let mut by_lib: BTreeMap<String, Vec<(String, u16)>> = BTreeMap::new();
    for (lib, name, ty) in &b.objects {
        let id = {
            let mut t = store.begin()?;
            let l = ops::find_library(&mut t, qsys, qsname::Name::parse(lib).map_err(|e| Error::NotPtf(e.to_string()))?)?.id;
            let found = store::obj::library::Library::load(&mut t, l)?
                .lookup(&mut t, *ty, qsname::Name::parse(name).map_err(|e| Error::NotPtf(e.to_string()))?)?;
            t.abort();
            found.ok_or_else(|| Error::NotPtf(format!("{lib}/{name} not found")))?
        };
        sign_object(store, id, b.key)?;
        by_lib.entry(lib.clone()).or_default().push((name.clone(), *ty));
    }
    let mut header = b.header.clone();
    header.object_count = u32::try_from(b.objects.len()).expect("objects");
    header.lic = b.lic.is_some();
    let mut w = Writer::new(Vec::new(), &media_header(&header.ptf, &b.system, now))?;
    w.parameters(&format!("QSCRTPTF PTF({})", header.ptf))?;
    let mut parts = BTreeMap::new();
    parts.insert(Part::typed(0), header.encode());
    if !b.cover.is_empty() {
        parts.insert(Part::typed(1), b.cover.as_bytes().to_vec());
    }
    pseudo(&mut w, b.key, HEADER_TYPE, &header.ptf, &parts)?;
    for (lib, objs) in &by_lib {
        let opts = SaveOptions {
            select: objs
                .iter()
                .map(|(n, t)| ObjSel {
                    name: n.clone(),
                    type_code: Some(*t),
                })
                .collect(),
            library_object: false,
            update_history: false,
            now,
            ..SaveOptions::default()
        };
        let rep = saverst::save_library(store, JobHandle(0), &AllowAll, qsys, lib, &opts, &mut w)?;
        if rep.not_saved() > 0 || rep.saved() != objs.len() {
            return Err(Error::NotPtf(format!("objects of {lib} not saved")));
        }
    }
    if let Some(uki) = b.lic {
        let mut parts = BTreeMap::new();
        parts.insert(Part::SPACE, uki.to_vec());
        if let Some(other) = b.lic_b {
            parts.insert(Part::typed(2), other.to_vec());
        }
        pseudo(&mut w, b.key, LIC_TYPE, "LIC", &parts)?;
    }
    Ok(w.finish(0)?.1)
}

/// A PTF opened from its stream.
#[derive(Debug, Clone)]
pub struct Package {
    /// The header.
    pub header: PtfHeader,
    /// The cover letter.
    pub cover: String,
    /// The replacement objects.
    pub objects: Vec<ObjRef>,
    /// The new UKI of a LIC PTF.
    pub lic: Option<Vec<u8>>,
    /// Slot B's build, when the PTF carries one.
    pub lic_b: Option<Vec<u8>>,
}

/// Open a PTF stream, checking that every object is signed by one of
/// the `trusted` keys and verifies.
pub fn open(bytes: &[u8], trusted: &[[u8; 32]]) -> Result<Package> {
    let mut r = Reader::new(Cursor::new(bytes))?;
    if r.header().kind != SaveKind::Ptf {
        return Err(Error::NotPtf("not a PTF stream".into()));
    }
    let mut header = None;
    let mut cover = String::new();
    let mut objects = Vec::new();
    let mut lic = None;
    let mut lic_b = None;
    while let Some(scan) = r.next_object()? {
        let Scan::Object(o) = scan else {
            return Err(Error::NotPtf("a damaged object".into()));
        };
        let d = &o.descriptor;
        let ok = o.signature.is_some_and(|s| {
            trusted
                .iter()
                .any(|k| sign::key_id(k) == s.key_id && sign::verify(k, &s.digest, &s.signature))
        });
        let what = d.object_name();
        if !ok {
            return Err(Error::Signature(what));
        }
        match d.type_code {
            HEADER_TYPE => {
                header = Some(PtfHeader::decode(o.part(Part::typed(0)))?);
                cover = String::from_utf8_lossy(o.part(Part::typed(1))).into_owned();
            }
            LIC_TYPE => {
                lic = Some(o.part(Part::SPACE).to_vec());
                let b = o.part(Part::typed(2));
                lic_b = (!b.is_empty()).then(|| b.to_vec());
            }
            _ if d.owner_index == savfmt::object::NO_OWNER => {
                objects.push((d.library_name(), what, d.type_code));
            }
            _ => {}
        }
    }
    let header = header.ok_or_else(|| Error::NotPtf("no PTF header".into()))?;
    if usize::try_from(header.object_count).ok() != Some(objects.len()) {
        return Err(Error::NotPtf("object count".into()));
    }
    Ok(Package {
        header,
        cover,
        objects,
        lic,
        lic_b,
    })
}

/// The header and cover letter of a PTF stream, signatures unchecked:
/// for a publisher listing what it signed itself.
pub fn peek(bytes: &[u8]) -> Result<(PtfHeader, String)> {
    let mut r = Reader::new(Cursor::new(bytes))?;
    if r.header().kind != SaveKind::Ptf {
        return Err(Error::NotPtf("not a PTF stream".into()));
    }
    while let Some(scan) = r.next_object()? {
        if let Scan::Object(o) = scan
            && o.descriptor.type_code == HEADER_TYPE
        {
            let h = PtfHeader::decode(o.part(Part::typed(0)))?;
            return Ok((h, String::from_utf8_lossy(o.part(Part::typed(1))).into_owned()));
        }
    }
    Err(Error::NotPtf("no PTF header".into()))
}
