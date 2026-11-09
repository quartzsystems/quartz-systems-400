// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's own TLS certificate and key, which its servers present
//! when no certificate is assigned to them: `QUSRSYS/QTVTLSCRT` (public
//! `*USE`) and `QUSRSYS/QTVTLSKEY` (public `*EXCLUDE`), owned by `QSYS`.
//! The service keeps them, so every server profile gets the same pair
//! whatever authority it has to `QUSRSYS`.

use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::library::Library;
use store::obj::{CreateParams, ObjectId, types};
use store::txn::Txn;

use crate::ipl::system::{System, now_unix};

const LIB: &str = "QUSRSYS";
const CERT: &str = "QTVTLSCRT";
const KEY: &str = "QTVTLSKEY";
const LEN: u16 = 2000;

/// A certificate and its key, PEM.
pub type Pair = (String, String);

/// The stored pair; when there is none, `made` is stored and given back
/// (`None` when nothing was made either). Only the system's server
/// profiles, or a user with `*ALLOBJ` and `*IOSYSCFG`, may have it.
pub fn identity(
    system: &mut System,
    principal: &Principal,
    made: Option<Pair>,
) -> Result<Option<Pair>, String> {
    if !super::certs::may_have_key(principal) {
        return Err(format!(
            "CPF2189 {} may not have the system's TLS key",
            principal.user
        ));
    }
    sec::guard::as_system(|| {
        let s = system.storage.as_mut().ok_or("object store not open")?;
        let qsys = s.qsys.ok_or("object store not initialized")?;
        crate::svc::db::with_txn(&mut s.store, |txn| {
            let lib = wm::obj::library(txn, qsys, LIB).map_err(|e| e.to_string())?;
            let l = Library::load(txn, lib).map_err(|e| e.to_string())?;
            let stored = read(txn, &l, CERT).zip(read(txn, &l, KEY));
            if stored.is_some() {
                return Ok(stored);
            }
            let Some((cert, key)) = made else {
                return Ok(None);
            };
            put(txn, &l, lib, CERT, &cert, Auth::USE)?;
            put(txn, &l, lib, KEY, &key, Auth::EXCLUDE)?;
            Ok(Some((cert, key)))
        })
    })
}

fn find(txn: &mut Txn<'_>, l: &Library, name: &str) -> Option<ObjectId> {
    let n = qsname::Name::parse(name).ok()?;
    l.lookup(txn, types::DTAARA, n).ok()?
}

/// An area's PEM; `None` when missing or holding none.
fn read(txn: &mut Txn<'_>, l: &Library, name: &str) -> Option<String> {
    let id = find(txn, l, name)?;
    match dtaara::read(txn, id).ok()? {
        DataAreaValue::Char(s) => {
            Some(s.trim_end().to_string()).filter(|t| t.contains("-----BEGIN"))
        }
        _ => None,
    }
}

/// Write an area, made when missing, with public authority `public`.
fn put(
    txn: &mut Txn<'_>,
    l: &Library,
    lib: ObjectId,
    name: &str,
    pem: &str,
    public: Auth,
) -> Result<(), String> {
    if pem.len() > usize::from(LEN) {
        return Err(format!(
            "{LIB}/{name}: the PEM is longer than the data area"
        ));
    }
    let id = if let Some(id) = find(txn, l, name) {
        id
    } else {
        let p = CreateParams {
            library: lib,
            name: qsname::Name::parse(name).map_err(|e| e.to_string())?,
            type_code: types::DTAARA,
            attribute: String::new(),
            text: "The system's TLS certificate".into(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: now_unix(),
            by_system: true,
        };
        dtaara::create(
            txn,
            &p,
            DataAreaType::Char { len: LEN },
            &DataAreaValue::Char(String::new()),
        )
        .map_err(|e| e.to_string())?
    };
    sec::auth::grant::set_public(txn, id, Some(public)).map_err(|e| e.to_string())?;
    dtaara::write(txn, id, &DataAreaValue::Char(pem.to_string())).map_err(|e| e.to_string())
}
