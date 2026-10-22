// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The store's guard. The storage service installs one [`SecGuard`] on
//! each store and runs every job request inside [`as_job`] with the
//! job's session; work the system does for itself (job start, spooling
//! into `QSPL`, the job log) runs inside [`as_system`] or with no
//! session, and is not checked. Failed checks are kept for the audit
//! journal ([`take_failures`]), since the request that failed usually
//! aborts its transaction.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use store::obj::guard::Guard;
use store::obj::header::{Authority as A, ObjectHeader, text};
use store::obj::{ObjectId, types};
use store::txn::Txn;
use store::{Result as StoreResult, StoreError};

use crate::auth::bits::Auth;
use crate::auth::cache::Session;
use crate::auth::required::{self, Need};
use crate::auth::search::{self, Decision, Step};
use crate::auth::{autl, grant};
use crate::profile::index::{self, Kind};
use crate::profile::objects;
use crate::rec::text_of;

/// A refused check, for the `AF` audit entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The user.
    pub user: String,
    /// The operation (`required.toml`).
    pub op: &'static str,
    /// The object.
    pub object: ObjectId,
    /// Its name.
    pub name: String,
    /// Its library's name.
    pub library: String,
    /// Its type, `*FILE`.
    pub type_name: String,
    /// What was found.
    pub have: Auth,
    /// The step that decided.
    pub step: Step,
}

thread_local! {
    static SESSION: RefCell<Option<Rc<RefCell<Session>>>> = const { RefCell::new(None) };
    static FAILURES: RefCell<Vec<Failure>> = const { RefCell::new(Vec::new()) };
}

/// Run `f` as the job whose session this is.
pub fn as_job<T>(session: &Rc<RefCell<Session>>, f: impl FnOnce() -> T) -> T {
    let prev = SESSION.with(|s| s.borrow_mut().replace(Rc::clone(session)));
    let out = f();
    SESSION.with(|s| *s.borrow_mut() = prev);
    out
}

/// Run `f` as the system: nothing is checked.
pub fn as_system<T>(f: impl FnOnce() -> T) -> T {
    let prev = SESSION.with(|s| s.borrow_mut().take());
    let out = f();
    SESSION.with(|s| *s.borrow_mut() = prev);
    out
}

/// The session in force, if any.
#[must_use]
pub fn current() -> Option<Rc<RefCell<Session>>> {
    SESSION.with(|s| s.borrow().clone())
}

/// The refused checks since the last call.
#[must_use]
pub fn take_failures() -> Vec<Failure> {
    FAILURES.with(|f| std::mem::take(&mut *f.borrow_mut()))
}

/// The guard. `sysvals` holds the security system values creation needs
/// (`QCRTAUT`, `QCRTOBJAUD`); the service refreshes it when they change.
pub struct SecGuard {
    qsys: Mutex<Option<ObjectId>>,
    sysvals: Mutex<HashMap<String, String>>,
}

impl SecGuard {
    /// A guard; `qsys` is unknown on a store being formatted.
    #[must_use]
    pub fn new(qsys: Option<ObjectId>) -> Arc<Self> {
        Arc::new(Self {
            qsys: Mutex::new(qsys),
            sysvals: Mutex::new(HashMap::new()),
        })
    }

    /// Learn `QSYS` (the temporary ASP's guard looks profiles up in ASP 1
    /// through the same ID).
    pub fn set_qsys(&self, qsys: ObjectId) {
        *self.qsys.lock().expect("qsys") = Some(qsys);
    }

    /// Set a system value's text.
    pub fn set_sysval(&self, name: &str, value: &str) {
        self.sysvals
            .lock()
            .expect("sysvals")
            .insert(name.into(), value.into());
    }

    fn sysval(&self, name: &str) -> String {
        self.sysvals
            .lock()
            .expect("sysvals")
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    fn qsys(&self) -> Option<ObjectId> {
        *self.qsys.lock().expect("qsys")
    }
}

fn denied(
    txn: &mut Txn<'_>,
    op: &'static str,
    h: &ObjectHeader,
    d: Decision,
    user: &str,
) -> StoreError {
    let type_name = types::type_by_code(h.id.type_code).map_or("*UNKNOWN", |t| t.name);
    let library = if h.id.type_code == types::LIB || h.id == h.library {
        "QSYS".to_string()
    } else {
        store::obj::ops::read_header(txn, h.library)
            .map_or_else(|_| String::new(), |(_, l)| l.name.to_string())
    };
    FAILURES.with(|f| {
        f.borrow_mut().push(Failure {
            user: user.into(),
            op,
            object: h.id,
            name: h.name.to_string(),
            library: library.clone(),
            type_name: type_name.into(),
            have: d.have,
            step: d.step,
        });
    });
    let msgid = if op.starts_with("lib.") {
        "CPF9820"
    } else {
        "CPF2189"
    };
    StoreError::NotAuthorized {
        name: h.name.to_string(),
        library,
        msgid,
        type_name: type_name.into(),
    }
}

/// Decide `op` on `h` for the session, using and filling its cache.
pub fn decide(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    s: &Rc<RefCell<Session>>,
    op: &'static str,
    h: &ObjectHeader,
) -> crate::Result<Decision> {
    let type_name = types::type_by_code(h.id.type_code).map_or("*UNKNOWN", |t| t.name);
    let need = required::need(op, type_name).unwrap_or(Need::Bits(Auth(Auth::MASK)));
    let (principal, cached) = {
        let mut sess = s.borrow_mut();
        let c = sess.get(h.id, h.auth_gen);
        (sess.principal.clone(), c)
    };
    let found = if let Some(f) = cached {
        f
    } else {
        let f = search::find(txn, qsys, &principal, h)?;
        s.borrow_mut().put(h.id, h.auth_gen, f);
        f
    };
    search::decide(txn, qsys, &principal, h, found, need)
}

fn to_store(e: crate::SecError) -> StoreError {
    match e {
        crate::SecError::Store(s) => s,
        other => StoreError::BadValue {
            what: other.to_string(),
        },
    }
}

impl Guard for SecGuard {
    fn check(&self, txn: &mut Txn<'_>, op: &'static str, h: &ObjectHeader) -> StoreResult<()> {
        let (Some(s), Some(qsys)) = (current(), self.qsys()) else {
            return Ok(());
        };
        let type_name = types::type_by_code(h.id.type_code).map_or("*UNKNOWN", |t| t.name);
        if required::need(op, type_name).is_none() {
            tracing::warn!("authority: operation {op} is not in required.toml");
        }
        let d = decide(txn, qsys, &s, op, h).map_err(to_store)?;
        if d.allowed {
            Ok(())
        } else {
            let user = s.borrow().principal.user.clone();
            Err(denied(txn, op, h, d, &user))
        }
    }

    fn creating(
        &self,
        txn: &mut Txn<'_>,
        h: &mut ObjectHeader,
        library: &ObjectHeader,
    ) -> StoreResult<()> {
        let Some(qsys) = self.qsys() else {
            return Ok(());
        };
        if let Some(s) = current() {
            let user = s.borrow().principal.user.clone();
            let mut owner = user.clone();
            if let Ok((_, p)) = objects::read(txn, qsys, &user) {
                if p.owner_grpprf && p.grpprf != "*NONE" {
                    owner.clone_from(&p.grpprf);
                } else if p.grpauttyp_pgp && p.grpprf != "*NONE" && p.grpaut != "*NONE" {
                    h.primary_group = text(&p.grpprf)?;
                    h.group_authority = Auth::parse(&[p.grpaut.as_str()], false)
                        .unwrap_or_default()
                        .store();
                }
                h.audit = text(&match self.sysval("QCRTOBJAUD").as_str() {
                    "*USRPRF" => p.objaud.clone(),
                    "" => "*NONE".into(),
                    v => v.to_string(),
                })?;
            }
            h.owner = text(&owner)?;
            h.creator = text(&user)?;
        } else if let v @ ("*CHANGE" | "*ALL") = self.sysval("QCRTOBJAUD").as_str() {
            h.audit = text(v)?;
        }
        // CRTAUT of the library, then QCRTAUT.
        let crtaut = if h.id.type_code == types::LIB || library.id.type_code != types::LIB {
            "*SYSVAL".to_string()
        } else {
            store::obj::library::Library::load(txn, library.id)?
                .attr
                .create_authority_text()
        };
        let crtaut = if crtaut == "*SYSVAL" || crtaut.is_empty() {
            let v = self.sysval("QCRTAUT");
            if v.is_empty() { "*CHANGE".into() } else { v }
        } else {
            crtaut
        };
        h.public_authority = match crtaut.as_str() {
            "*ALL" => A::ALL,
            "*USE" => Auth::USE.store(),
            "*EXCLUDE" => A::EXCLUDE,
            "*CHANGE" => Auth::CHANGE.store(),
            list => {
                // An authorization list's name: secured by it, public from it.
                if autl::find(txn, qsys, list).is_ok() {
                    h.autl = text(list)?;
                    A(A::FROM_AUTL)
                } else {
                    Auth::CHANGE.store()
                }
            }
        };
        Ok(())
    }

    fn created(&self, txn: &mut Txn<'_>, h: &ObjectHeader) -> StoreResult<()> {
        let Some(qsys) = self.qsys() else {
            return Ok(());
        };
        let r = (|| -> crate::Result<()> {
            let owner = text_of(&h.owner);
            index::put(txn, qsys, &owner, Kind::Owned, h.id, 0)?;
            let pgp = text_of(&h.primary_group);
            if !pgp.is_empty() {
                index::put(
                    txn,
                    qsys,
                    &pgp,
                    Kind::Group,
                    h.id,
                    u64::from(h.group_authority.0),
                )?;
            }
            let list = text_of(&h.autl);
            if !list.is_empty() {
                autl::secure(txn, qsys, &list, h.id)?;
            }
            // GRPAUT given privately to the creator's group.
            if let Some(s) = current() {
                let user = s.borrow().principal.user.clone();
                if let Ok((_, p)) = objects::read(txn, qsys, &user)
                    && !p.owner_grpprf
                    && !p.grpauttyp_pgp
                    && p.grpprf != "*NONE"
                    && p.grpaut != "*NONE"
                {
                    let bits = Auth::parse(&[p.grpaut.as_str()], h.id.type_code == types::AUTL)
                        .unwrap_or_default();
                    grant::set_private(txn, qsys, h.id, &p.grpprf, Some(bits))?;
                }
            }
            Ok(())
        })();
        r.map_err(to_store)
    }

    fn deleting(&self, txn: &mut Txn<'_>, h: &ObjectHeader) -> StoreResult<()> {
        let Some(qsys) = self.qsys() else {
            return Ok(());
        };
        let r = (|| -> crate::Result<()> {
            let owner = text_of(&h.owner);
            index::remove(txn, qsys, &owner, Kind::Owned, h.id)?;
            if h.id.type_code != types::USRPRF && h.id.type_code != types::AUTL {
                let size = i64::try_from(h.size_pages).unwrap_or(0) * grant::PAGE_KB;
                grant::charge(txn, qsys, &owner, -size)?;
            }
            let pgp = text_of(&h.primary_group);
            if !pgp.is_empty() {
                index::remove(txn, qsys, &pgp, Kind::Group, h.id)?;
            }
            for pa in store::obj::auth::private_list(txn, h)? {
                index::remove(txn, qsys, &text_of(&pa.profile), Kind::Private, h.id)?;
            }
            let list = text_of(&h.autl);
            if !list.is_empty() {
                autl::unsecure(txn, qsys, &list, h.id)?;
            }
            if h.id.type_code == types::USRPRF || h.id.type_code == types::AUTL {
                crate::auth::cache::bump_epoch();
            }
            Ok(())
        })();
        r.map_err(to_store)
    }

    fn sizing(&self, txn: &mut Txn<'_>, h: &ObjectHeader, pages: i64) -> StoreResult<()> {
        let Some(qsys) = self.qsys() else {
            return Ok(());
        };
        if h.id.type_code == types::USRPRF || h.id.type_code == types::AUTL || h.temporary {
            return Ok(());
        }
        grant::charge(txn, qsys, &text_of(&h.owner), pages * grant::PAGE_KB).map_err(to_store)
    }
}
