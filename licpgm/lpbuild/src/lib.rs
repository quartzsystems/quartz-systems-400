// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Product loads made at build time (`build lp`, `docs/LICPGM.md` §6):
//! a store in memory gets the product's objects (for `QS00SS1` what
//! `osobjects` makes; for the others what their manifest names), its
//! `*PRDDFN` and packaged `*PRDLOD`s, every object signed with the
//! build key, and the whole saved as a `SaveKind::LicPgm` stream, the
//! bytes a save file holds for RSTLICPGM.

#![deny(missing_docs)]

use std::cell::RefCell;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ed25519_dalek::SigningKey;
use licpgm::plan::Objects;
use licpgm::plan::save::{SavLicPgm, plan_save};
use licpgm::{LoadState, ProductStore, QualName, pkg};
use lpstore::Products;
use qsname::Name;
use saverst::AllowAll;
use savfmt::{Compression, MediaHeader, SaveKind};
use store::obj::header::Signature;
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};
use thiserror::Error;

/// The libraries an empty system has (`lic/slic`'s first IPL makes the
/// same).
pub const SYSTEM_LIBRARIES: &[&str] = &[
    "QGPL",
    "QUSRSYS",
    "QSYS2",
    "QRECOVERY",
    "QSPL",
    "QRCL",
    "QHLPSYS",
];

/// A product load that could not be built.
#[derive(Debug, Error)]
pub enum BuildError {
    /// The manifest has errors.
    #[error("manifest: {0}")]
    Manifest(String),
    /// The store refused something.
    #[error("store: {0}")]
    Store(String),
    /// The OS objects could not be made.
    #[error(transparent)]
    Os(#[from] osobjects::Error),
}

fn store_err(e: impl std::fmt::Display) -> BuildError {
    BuildError::Store(e.to_string())
}

/// A built product.
#[derive(Debug)]
pub struct Built {
    /// The product.
    pub product: String,
    /// The save stream.
    pub stream: Vec<u8>,
    /// Objects signed.
    pub signed: usize,
}

/// An empty system in memory: `QSYS` and the system libraries.
fn empty(now: u64) -> Result<(Store, ObjectId), BuildError> {
    let d = Arc::new(MemBlockDev::with_seed(262_144, 11));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [1; 16],
        unit_uuids: vec![[2; 16]],
        unit_serials: Vec::new(),
        wal_pages: 4096,
        now,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .map_err(store_err)?;
    let mut t = store.begin().map_err(store_err)?;
    let qsys = ops::bootstrap(&mut t, now, "QS400").map_err(store_err)?;
    for lib in SYSTEM_LIBRARIES {
        ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: Name::parse(lib).expect("a name"),
                type_code: types::LIB,
                attribute: String::new(),
                text: format!("System library {lib}"),
                owner: "QSYS".into(),
                system: "QS400".into(),
                now,
                by_system: true,
            },
        )
        .map_err(store_err)?;
    }
    t.commit().map_err(store_err)?;
    Ok((store, qsys))
}

/// Sign every object in the store but the libraries (members too,
/// which no library lists): the digest is taken with the signed flag
/// already set, as a restore checks it.
fn sign_all(store: &mut Store, key: &SigningKey) -> Result<usize, BuildError> {
    let public = key.verifying_key().to_bytes();
    let key_id = saverst::sign::key_id(&public);
    let mut t = store.begin().map_err(store_err)?;
    let mut n = 0;
    {
        for (id, _) in store::obj::dir::objdir_entries(&mut t).map_err(store_err)? {
            if id.type_code == types::LIB {
                continue;
            }
            let (page, mut h) = ops::read_header(&mut t, id).map_err(store_err)?;
            h.signature = Some(Signature {
                bytes: [0; 64],
                key_id,
                system: true,
            });
            ops::write_header(&mut t, page, &h).map_err(store_err)?;
            let digest = saverst::sign::content_digest(&mut t, id).map_err(store_err)?;
            let (page, mut h) = ops::read_header(&mut t, id).map_err(store_err)?;
            h.signature = Some(Signature {
                bytes: saverst::sign::sign(key, &digest),
                key_id,
                system: true,
            });
            ops::write_header(&mut t, page, &h).map_err(store_err)?;
            n += 1;
        }
    }
    t.commit().map_err(store_err)?;
    Ok(n)
}

fn header(now: u64, product: &str) -> MediaHeader {
    let r = release::RELEASE;
    let mut id = [0u8; 16];
    id[..7].copy_from_slice(&product.as_bytes()[..7]);
    MediaHeader {
        release: (r.v, r.r, r.m),
        target: (r.v, r.r, r.m),
        compression: Compression::None,
        kind: SaveKind::LicPgm,
        created: now,
        stream_id: id,
        system: "QS400".into(),
        serial: String::new(),
        command: "SAVLICPGM".into(),
        user: "QSYS".into(),
    }
}

/// Build a product's load from its manifest. `QS00SS1`'s objects are
/// what `osobjects` makes; its `*BASE` code load lists them.
pub fn build(manifest: &str, key: &SigningKey, now: u64) -> Result<Built, BuildError> {
    let opts = pkg::Options {
        allow_quartz: true,
        catalog: false,
    };
    let pkg = pkg::parse(manifest, opts).map_err(|errs| {
        BuildError::Manifest(
            errs.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let (mut store, qsys) = empty(now)?;
    let product = pkg.definition.product.clone();
    let is_os = product.as_str() == licpgm::ProductId::OS;
    if is_os {
        let stamp = lpstore::stamp(now);
        let mut t = store.begin().map_err(store_err)?;
        osobjects::create(&mut t, qsys, &stamp)?;
        t.commit().map_err(store_err)?;
    }
    let libs: Vec<String> = {
        let mut v = vec!["QSYS".to_string()];
        for (_, l) in &pkg.loads {
            for e in &l.libraries {
                if !v.contains(&e.installed) {
                    v.push(e.installed.clone());
                }
            }
        }
        v
    };
    {
        let cell = RefCell::new(&mut store);
        let mut p = Products::new(&cell, qsys, lpstore::stamp(now));
        for lib in &libs {
            if !p.library_exists(lib) {
                return Err(BuildError::Store(format!("library {lib} does not exist")));
            }
        }
        p.create_definition(
            &QualName::new(&pkg.definition_name.name, "QSYS"),
            &pkg.definition,
        )
        .map_err(store_err)?;
        let objs = Products::new(&cell, qsys, lpstore::stamp(now));
        for (q, l) in &pkg.loads {
            let mut l = l.clone();
            // The OS base code load is every object the OS made.
            if is_os && l.key.option.is_base() && l.key.load_type == licpgm::LoadType::Code {
                for e in &l.libraries {
                    // Libraries are the system's, not the product's.
                    l.objects.extend(
                        licpgm::ObjectCheck::objects_in(&objs, &e.installed)
                            .into_iter()
                            .filter(|o| o.typ != "*LIB"),
                    );
                }
            }
            l.state = LoadState::Packaged;
            p.create_load(&QualName::new(&q.name, "QSYS"), &l)
                .map_err(store_err)?;
        }
    }
    let signed = sign_all(&mut store, key)?;
    let cell = RefCell::new(&mut store);
    let p = Products::new(&cell, qsys, lpstore::stamp(now));
    let req = SavLicPgm {
        product: product.clone(),
        option: None,
        release: Some(pkg.definition.release),
        lng: None,
        objects: Objects::All,
    };
    let plan = plan_save(&p, &req).map_err(store_err)?;
    let stream = lpstore::save_product(
        &cell,
        qsys,
        JobHandle(1),
        &AllowAll,
        &plan,
        &header(now, product.as_str()),
        &format!("SAVLICPGM LICPGM({product}) OPTION(*ALL)"),
    )
    .map_err(BuildError::Store)?;
    Ok(Built {
        product: product.to_string(),
        stream,
        signed,
    })
}
