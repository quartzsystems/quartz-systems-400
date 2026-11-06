// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The profile exit program flags `QWTSETPX` sets and `QWTRTVPX`
//! returns: for each profile, which of the eight programs of
//! `QIBM_QWT_PREATTNPGMS` (`ATTN0100`) and `QIBM_QWT_SYSREQPGMS`
//! (`SREQ0100`) its jobs call. They live in the user space
//! `QUSRSYS/QWTPRFEXIT`, shipped `*PUBLIC *EXCLUDE` and read and
//! written here as the system: a text payload, one line per profile,
//! `USER ATTN0100 01000000 SREQ0100 00000000` after a `QPRFEXIT 1`
//! header. A profile without a line calls none, as IBM's do until
//! `QWTSETPX` is used.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use qsname::Name;
use store::obj::{CreateParams, ObjectId, ops, types, usrspc};
use store::txn::Txn;

use crate::error::{Result, SecError};

/// The space's library and name.
pub const SPACE: (&str, &str) = ("QUSRSYS", "QWTPRFEXIT");
/// The payload's first line.
const HEADER: &str = "QPRFEXIT 1";
/// The two formats.
pub const FORMATS: [&str; 2] = ["ATTN0100", "SREQ0100"];

/// Every profile's flags, by profile and format.
pub type Flags = BTreeMap<String, BTreeMap<String, [bool; 8]>>;

fn find(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Option<ObjectId>> {
    let lib = ops::find_library(txn, qsys, Name::parse(SPACE.0).expect("valid"))?.id;
    let name = Name::parse(SPACE.1).expect("valid");
    Ok(ops::list(txn, lib, Some(types::USRSPC))?
        .into_iter()
        .find(|(k, _)| k.name == name)
        .map(|(_, id)| id))
}

fn decode(text: &str) -> Flags {
    let mut out = Flags::new();
    for line in text.lines().skip(1) {
        let mut w = line.split_whitespace();
        let Some(user) = w.next() else { continue };
        let mut per = BTreeMap::new();
        while let (Some(f), Some(bits)) = (w.next(), w.next()) {
            let mut b = [false; 8];
            for (i, c) in bits.chars().take(8).enumerate() {
                b[i] = c == '1';
            }
            per.insert(f.to_string(), b);
        }
        out.insert(user.to_string(), per);
    }
    out
}

fn encode(flags: &Flags) -> String {
    let mut s = format!("{HEADER}\n");
    for (user, per) in flags {
        s.push_str(user);
        for (f, b) in per {
            let bits: String = b.iter().map(|x| if *x { '1' } else { '0' }).collect();
            let _ = write!(s, " {f} {bits}");
        }
        s.push('\n');
    }
    s
}

/// Every profile's flags.
pub fn load(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Flags> {
    crate::guard::as_system(|| {
        let Some(id) = find(txn, qsys)? else {
            return Ok(Flags::new());
        };
        let head = usrspc::read(txn, id, 0, 4)?;
        let n = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let body = usrspc::read(txn, id, 4, n)?;
        Ok(decode(&String::from_utf8_lossy(&body)))
    })
}

/// Write every profile's flags, making the space the first time.
pub fn save(txn: &mut Txn<'_>, qsys: ObjectId, flags: &Flags, now: u64) -> Result<()> {
    crate::guard::as_system(|| {
        let text = encode(flags);
        let mut data = u32::try_from(text.len())
            .map_err(|_| SecError::msg("CPF3CF2", "too many profile exit settings"))?
            .to_be_bytes()
            .to_vec();
        data.extend_from_slice(text.as_bytes());
        let id = if let Some(id) = find(txn, qsys)? {
            id
        } else {
            let lib = ops::find_library(txn, qsys, Name::parse(SPACE.0).expect("valid"))?.id;
            usrspc::create(
                txn,
                &CreateParams {
                    library: lib,
                    name: Name::parse(SPACE.1).expect("valid"),
                    type_code: types::USRSPC,
                    attribute: String::new(),
                    text: "Profile exit program settings".into(),
                    owner: "QSYS".into(),
                    system: String::new(),
                    now,
                    by_system: true,
                },
                4096,
                true,
                0,
            )?
        };
        usrspc::write(txn, id, 0, &data)?;
        Ok(())
    })
}

/// `user`'s flags for `format`, all off when never set.
#[must_use]
pub fn of(flags: &Flags, user: &str, format: &str) -> [bool; 8] {
    flags
        .get(user)
        .and_then(|p| p.get(format))
        .copied()
        .unwrap_or([false; 8])
}

/// Apply `QWTSETPX`'s values: 1 call, 0 do not, -1 leave as it is.
pub fn set(flags: &mut Flags, user: &str, format: &str, values: &[i32]) {
    let mut b = of(flags, user, format);
    for (i, v) in values.iter().take(8).enumerate() {
        match v {
            1 => b[i] = true,
            0 => b[i] = false,
            _ => {}
        }
    }
    flags
        .entry(user.to_string())
        .or_default()
        .insert(format.to_string(), b);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_keeps_minus_one_and_round_trips() {
        let mut f = Flags::new();
        set(&mut f, "QSECOFR", "ATTN0100", &[1, 0, 1]);
        set(&mut f, "QSECOFR", "ATTN0100", &[-1, 1]);
        assert_eq!(of(&f, "QSECOFR", "ATTN0100")[..3], [true, true, true]);
        assert_eq!(of(&f, "QSECOFR", "SREQ0100"), [false; 8]);
        let back = decode(&encode(&f));
        assert_eq!(back, f);
    }
}
