// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Profiles as objects: `QSYS/name *USRPRF`, the payload in the space
//! behind a length prefix.

use qsname::Name;
use store::obj::header::Authority;
use store::obj::ops::{self, CreateParams};
use store::obj::space::Space;
use store::obj::{ObjectId, auth, types};
use store::txn::Txn;

use crate::profile::payload::UsrPrf;
use crate::{Result, SecError};

/// `CPF2204`: the profile does not exist.
pub fn not_found(name: &str) -> SecError {
    SecError::msg("CPF2204", format!("User profile {name} not found."))
}

fn parse(name: &str) -> Result<Name> {
    Name::parse(name).map_err(|_| {
        SecError::msg(
            "CPF2203",
            format!("{name} is not a valid user profile name."),
        )
    })
}

/// The profile object `name`, if it exists.
pub fn find(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<Option<ObjectId>> {
    let n = parse(name)?;
    let lib = store::obj::library::Library::load(txn, qsys)?;
    Ok(lib.lookup(txn, types::USRPRF, n)?)
}

/// Whether `name` is a profile.
pub fn exists(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> bool {
    matches!(find(txn, qsys, name), Ok(Some(_)))
}

/// Read a profile's payload.
pub fn read_id(txn: &mut Txn<'_>, id: ObjectId) -> Result<UsrPrf> {
    let (space, _) = Space::load(txn, id)?;
    let head = space.read(txn, 0, 4)?;
    let n = u32::from_le_bytes(head[..4].try_into().expect("4")) as usize;
    if 4 + n as u64 > space.bytes() {
        return Err(SecError::Corrupt(
            "profile payload runs past its space".into(),
        ));
    }
    UsrPrf::decode(&space.read(txn, 4, n)?)
}

/// Read profile `name`: its object and payload.
pub fn read(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<(ObjectId, UsrPrf)> {
    let id = find(txn, qsys, name)?.ok_or_else(|| not_found(name))?;
    Ok((id, read_id(txn, id)?))
}

/// Replace a profile's payload.
pub fn write(txn: &mut Txn<'_>, id: ObjectId, p: &UsrPrf) -> Result<()> {
    let payload = p.encode();
    let mut bytes = u32::try_from(payload.len())
        .map_err(|_| SecError::Corrupt("profile over 4 GiB".into()))?
        .to_le_bytes()
        .to_vec();
    bytes.extend_from_slice(&payload);
    let space = Space::ensure(txn, id, bytes.len() as u64, 0)?;
    space.write(txn, 0, &bytes)?;
    Ok(())
}

/// Create profile object `p.name` owned by `owner`, `*PUBLIC` as `public`.
pub fn create(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    p: &UsrPrf,
    owner: &str,
    public: Authority,
    now: u64,
) -> Result<ObjectId> {
    if find(txn, qsys, &p.name)?.is_some() {
        return Err(SecError::msg(
            "CPF2214",
            format!("User profile {} already exists.", p.name),
        ));
    }
    let id = ops::create(
        txn,
        &CreateParams {
            library: qsys,
            name: parse(&p.name)?,
            type_code: types::USRPRF,
            attribute: String::new(),
            text: p.text.chars().take(50).collect(),
            owner: owner.into(),
            system: "QS400".into(),
            now,
            by_system: true,
        },
    )?;
    write(txn, id, p)?;
    let (page, mut h) = auth::load(txn, id)?;
    h.public_authority = public;
    auth::save_authority(txn, page, &mut h)?;
    Ok(id)
}

/// Every profile name, in EBCDIC order.
pub fn list(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<String>> {
    let lib = store::obj::library::Library::load(txn, qsys)?;
    Ok(lib
        .list(txn, Some(types::USRPRF))?
        .into_iter()
        .map(|(k, _)| k.name.to_string())
        .collect())
}
