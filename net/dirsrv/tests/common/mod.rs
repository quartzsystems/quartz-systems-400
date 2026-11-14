// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A store in memory with `QUSRDIRDB` and an instance store in it.

#![allow(dead_code)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use dirsrv::store::{AsciiFold, Attr, CreateOpts, DirDb, Entry};
use ldap::{Ava, Dn, Rdn};
use qsname::Name;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

pub const NOW: u64 = 1_767_225_600;
pub const NORM: AsciiFold = AsciiFold;

pub struct Sys {
    pub dev: Arc<MemBlockDev>,
    pub store: Store,
    pub qsys: ObjectId,
    pub lib: ObjectId,
    pub db: DirDb,
}

pub fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

pub fn lib_params(qsys: ObjectId, n: &str) -> CreateParams {
    CreateParams {
        library: qsys,
        name: name(n),
        type_code: types::LIB,
        attribute: String::new(),
        text: String::new(),
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: true,
    }
}

pub fn opts() -> CreateOpts {
    CreateOpts {
        text: "Directory server instance".into(),
        owner: "QDIRSRV".into(),
        system: "QS400".into(),
        now: NOW,
    }
}

/// A formatted store of `blocks` pages with `QUSRDIRDB/QUSRDIR`.
pub fn system(blocks: u64) -> Sys {
    let dev = Arc::new(MemBlockDev::with_seed(blocks, 7));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 1024,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(dev.clone()) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let lib = ops::create(&mut t, &lib_params(qsys, "QUSRDIRDB")).unwrap();
    let db = DirDb::create(&mut t, lib, name("QUSRDIR"), &opts()).unwrap();
    t.commit().unwrap();
    store.asp_superblock_set_qsys(qsys);
    Sys {
        dev,
        store,
        qsys,
        lib,
        db,
    }
}

/// An RFC 4514 DN of simple `type=value` RDNs (no escapes).
pub fn dn(s: &str) -> Dn {
    Dn(s.split(',').map(rdn).collect())
}

/// One RDN, `type=value` or `a=1+b=2`.
pub fn rdn(s: &str) -> Rdn {
    Rdn(s
        .trim()
        .split('+')
        .map(|ava| {
            let (t, v) = ava.split_once('=').unwrap();
            Ava {
                ty: t.trim().into(),
                value: v.trim().as_bytes().to_vec(),
            }
        })
        .collect())
}

/// A person entry named `cn=<cn>`.
pub fn person(cn: &str) -> Entry {
    Entry::new(
        &format!("cn={cn}"),
        vec![
            Attr::text("objectClass", &["top", "person"]),
            Attr::text("cn", &[cn]),
            Attr::text("sn", &[&format!("S{cn}")]),
        ],
    )
}

/// An organizational unit entry.
pub fn ou(name: &str) -> Entry {
    Entry::new(
        &format!("ou={name}"),
        vec![
            Attr::text("objectClass", &["top", "organizationalUnit"]),
            Attr::text("ou", &[name]),
        ],
    )
}

/// A suffix root entry.
pub fn root(suffix: &str) -> Entry {
    Entry::new(suffix, vec![Attr::text("objectClass", &["top", "domain"])])
}
