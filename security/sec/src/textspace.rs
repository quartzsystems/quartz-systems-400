// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The small security records QS/400 keeps as text in a user space of
//! `QUSRSYS`, shipped `*PUBLIC *EXCLUDE` and read and written here as
//! the system: a four-byte big-endian length, then UTF-8 lines, the
//! first a header naming the record and its version (`QFCNUSG 1`).
//! The profile exit flags ([`crate::prfexit`]), the function usage
//! registry ([`crate::fcnusg`]) and the security attributes
//! ([`crate::seca`]) live this way.

use qsname::Name;
use store::obj::{CreateParams, ObjectId, ops, types, usrspc};
use store::txn::Txn;

use crate::error::{Result, SecError};

/// The library of the records.
pub const LIB: &str = "QUSRSYS";

fn find(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<Option<ObjectId>> {
    let lib = ops::find_library(txn, qsys, Name::parse(LIB).expect("valid"))?.id;
    let name = Name::parse(name).map_err(|e| SecError::msg("CPF3C3C", e.to_string()))?;
    Ok(ops::list(txn, lib, Some(types::USRSPC))?
        .into_iter()
        .find(|(k, _)| k.name == name)
        .map(|(_, id)| id))
}

/// The record's lines after its header, `None` when there is no record
/// or its header is not `header`.
pub fn load(txn: &mut Txn<'_>, qsys: ObjectId, name: &str, header: &str) -> Result<Option<String>> {
    crate::guard::as_system(|| {
        let Some(id) = find(txn, qsys, name)? else {
            return Ok(None);
        };
        let head = usrspc::read(txn, id, 0, 4)?;
        let n = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let body = String::from_utf8_lossy(&usrspc::read(txn, id, 4, n)?).into_owned();
        let mut lines = body.splitn(2, '\n');
        if lines.next() != Some(header) {
            return Ok(None);
        }
        Ok(Some(lines.next().unwrap_or_default().to_string()))
    })
}

/// Write the record, making its space the first time.
pub fn save(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    (name, text): (&str, &str),
    header: &str,
    body: &str,
    now: u64,
) -> Result<()> {
    crate::guard::as_system(|| {
        let all = format!("{header}\n{body}");
        let mut data = u32::try_from(all.len())
            .map_err(|_| SecError::msg("CPF3CF2", format!("{name} is too large")))?
            .to_be_bytes()
            .to_vec();
        data.extend_from_slice(all.as_bytes());
        let id = if let Some(id) = find(txn, qsys, name)? {
            id
        } else {
            let lib = ops::find_library(txn, qsys, Name::parse(LIB).expect("valid"))?.id;
            let id = usrspc::create(
                txn,
                &CreateParams {
                    library: lib,
                    name: Name::parse(name).map_err(|e| SecError::msg("CPF3C3C", e.to_string()))?,
                    type_code: types::USRSPC,
                    attribute: String::new(),
                    text: text.into(),
                    owner: "QSYS".into(),
                    system: String::new(),
                    now,
                    by_system: true,
                },
                4096,
                true,
                0,
            )?;
            crate::auth::grant::set_public(txn, id, Some(crate::auth::bits::Auth::EXCLUDE))?;
            id
        };
        usrspc::write(txn, id, 0, &data)?;
        Ok(())
    })
}

/// Now, Unix seconds.
#[must_use]
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
