// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The work management objects in the store: each is an object of its
//! type whose space holds a length and the encoded payload.

pub mod cls;
pub mod jobd;
pub mod params;
pub mod shipped;

use qsname::{LibSpec, Name};
use store::obj::ops::{self, NameContext};
use store::obj::space::Space;
use store::obj::{CreateParams, ObjectId};
use store::txn::Txn;

use crate::{Error, Result};

/// Who and when, for a new object.
#[derive(Clone, Debug)]
pub struct Stamp {
    /// Owner.
    pub owner: String,
    /// System name.
    pub system: String,
    /// Unix time.
    pub now: u64,
}

/// Create `lib/name` of `type_code` holding `payload`.
pub fn create(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    type_code: u16,
    text: &str,
    stamp: &Stamp,
    payload: &[u8],
) -> Result<ObjectId> {
    let id = ops::create(
        txn,
        &CreateParams {
            library,
            name,
            type_code,
            attribute: String::new(),
            text: text.into(),
            owner: stamp.owner.clone(),
            system: stamp.system.clone(),
            now: stamp.now,
            by_system: true,
        },
    )?;
    write(txn, id, payload)?;
    Ok(id)
}

/// Replace an object's payload.
pub fn write(txn: &mut Txn<'_>, id: ObjectId, payload: &[u8]) -> Result<()> {
    let mut bytes = u32::try_from(payload.len())
        .map_err(|_| Error::Corrupt("payload over 4 GiB".into()))?
        .to_le_bytes()
        .to_vec();
    bytes.extend_from_slice(payload);
    // The length prefix says where the payload ends; a longer space is fine.
    let space = Space::ensure(txn, id, bytes.len() as u64, 0)?;
    space.write(txn, 0, &bytes)?;
    Ok(())
}

/// An object's payload.
pub fn read(txn: &mut Txn<'_>, id: ObjectId) -> Result<Vec<u8>> {
    let (space, _) = Space::load(txn, id)?;
    let head = space.read(txn, 0, 4)?;
    let n = u32::from_le_bytes(head[..4].try_into().expect("sized")) as usize;
    if 4 + n as u64 > space.bytes() {
        return Err(Error::Corrupt(format!(
            "payload of {n} bytes runs past its space"
        )));
    }
    Ok(space.read(txn, 4, n)?)
}

/// `lib/name` of a type: `*LIBL` and `*CURLIB` through `names`.
pub fn find(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &NameContext,
    lib: &str,
    name: &str,
    type_code: u16,
) -> Result<ObjectId> {
    let n = Name::parse(name).map_err(|e| Error::value("CPD0078", format!("{name}: {e}")))?;
    let spec = match lib.to_ascii_uppercase().as_str() {
        "*LIBL" | "" => LibSpec::Libl,
        "*CURLIB" => LibSpec::Curlib,
        l => {
            LibSpec::Name(Name::parse(l).map_err(|e| Error::value("CPD0078", format!("{l}: {e}")))?)
        }
    };
    Ok(ops::resolve(txn, qsys, names, spec, n, type_code)?.id)
}

/// A library by name.
pub fn library(txn: &mut Txn<'_>, qsys: ObjectId, lib: &str) -> Result<ObjectId> {
    if lib.eq_ignore_ascii_case("QSYS") {
        return Ok(qsys);
    }
    let n = Name::parse(lib).map_err(|e| Error::value("CPD0078", format!("{lib}: {e}")))?;
    Ok(ops::find_library(txn, qsys, n)?.id)
}

/// A one-byte count of names and the names, after a fixed part.
pub fn put_names(out: &mut Vec<u8>, names: &[String]) {
    out.extend_from_slice(&u16::try_from(names.len()).unwrap_or(0).to_le_bytes());
    for n in names {
        out.extend(crate::rec::text_bytes(n, 10));
    }
}

/// Read what [`put_names`] wrote; the names and the bytes used.
pub fn get_names(b: &[u8]) -> Result<(Vec<String>, usize)> {
    if b.len() < 2 {
        return Err(Error::Corrupt("name list truncated".into()));
    }
    let n = usize::from(u16::from_le_bytes([b[0], b[1]]));
    let end = 2 + n * 10;
    if b.len() < end {
        return Err(Error::Corrupt("name list truncated".into()));
    }
    let names = b[2..end].chunks(10).map(crate::rec::text_of).collect();
    Ok((names, end))
}
