// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Access for Web's stores (`net/accessweb`, the `ACCESSWEB` job): user
//! indexes `QUSRSYS/QZXWPOL` (policies) and `QZXWPRF` (preferences),
//! which `QS00AFW` ships owned by the system and `*PUBLIC *USE`. Only
//! the product's own job changes them, through `web_store`, here as the
//! system. An entry is the key, the profile in 10 bytes and the function
//! or preference key in 20, then the value.

use qsname::Name;
use store::obj::dtaq::KeyOp;
use store::obj::{ops, types, usridx};

use crate::ipl::system::System;

/// The job and its profile that may use the stores.
const JOB: (&str, &str) = ("ACCESSWEB", "QTMHHTTP");
/// The profile part of a key.
const WHO: usize = 10;
/// The whole key.
const KEY: usize = 30;

fn key(who: &str, what: &str) -> Vec<u8> {
    format!("{who:<WHO$}{what:<20}").into_bytes()
}

/// `web_store` from job `job` (name, user).
pub fn store(
    system: &mut System,
    job: (&str, &str),
    index: &str,
    who: &str,
    put: Option<Vec<(String, String)>>,
) -> Result<Vec<(String, String)>, String> {
    if job != JOB {
        return Err("CPF2207".into());
    }
    if !osobjects::products::AFW_STORES
        .iter()
        .any(|(n, _)| *n == index)
    {
        return Err("CPF9801".into());
    }
    let who = who.trim().to_ascii_uppercase();
    if who.is_empty() || who.len() > WHO || !who.is_ascii() {
        return Err("CPF9899".into());
    }
    let s = system.storage.as_mut().ok_or("CPF3CF2")?;
    let qsys = s.qsys.ok_or("CPF3CF2")?;
    sec::guard::as_system(|| {
        crate::svc::db::with_txn(&mut s.store, |txn| {
            let lib = ops::find_library(txn, qsys, Name::parse("QUSRSYS").expect("valid"))
                .map_err(|e| e.to_string())?
                .id;
            let name = Name::parse(index).map_err(|e| e.to_string())?;
            let id = ops::list(txn, lib, Some(types::USRIDX))
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|(k, _)| k.name == name)
                .map(|(_, id)| id)
                .ok_or("CPF9801")?;
            let mine = |e: &Vec<u8>| e.len() >= KEY && e[..WHO] == key(&who, "")[..WHO];
            let found: Vec<Vec<u8>> = usridx::find(txn, id, KeyOp::Ge, &key(&who, ""), usize::MAX)
                .map_err(|e| e.to_string())?
                .into_iter()
                .take_while(mine)
                .collect();
            let Some(values) = put else {
                return Ok(found
                    .iter()
                    .map(|e| {
                        let k = String::from_utf8_lossy(&e[WHO..KEY]).trim_end().to_string();
                        (k, String::from_utf8_lossy(&e[KEY..]).into_owned())
                    })
                    .collect());
            };
            for e in &found {
                usridx::remove(txn, id, KeyOp::Eq, &e[..KEY], 1).map_err(|e| e.to_string())?;
            }
            for (k, v) in &values {
                if k.len() > KEY - WHO
                    || !k.is_ascii()
                    || usize::from(osobjects::products::AFW_ENTRY) < KEY + v.len()
                {
                    continue;
                }
                let mut e = key(&who, k);
                e.extend_from_slice(v.as_bytes());
                usridx::insert(txn, id, &e).map_err(|e| e.to_string())?;
            }
            Ok(values)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(key("WEBUSER", "db.write").len(), KEY);
        assert_eq!(&key("WEBUSER", "db.write")[..12], b"WEBUSER   db");
    }
}
