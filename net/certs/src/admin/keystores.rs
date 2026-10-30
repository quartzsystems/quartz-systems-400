// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSCRTCRTS` and `QSDLTCRTS`: stores, their stashes, and the local CA.

use super::{Ctx, Keystore, Note, msgs};
use crate::apps::AppRegistry;
use crate::ca;
use crate::crypto::keys::KeyAlgorithm;
use crate::error::{CertError, Result};
use crate::paths::{LOCAL_CA_CERT, LOCAL_CA_STORE, stash_path};
use crate::roots;
use crate::stash;
use crate::store::{CertStore, StoreKind};
use crate::x509::build::Subject;

/// Create a store. With `stash` its password is sealed beside it so
/// servers and later commands open it without one; with `seed` the
/// shipped root CAs are added as trusted.
pub fn create(ctx: &Ctx<'_>, ks: &Keystore, stash: bool, seed: bool) -> Result<Vec<Note>> {
    if ks.path == LOCAL_CA_STORE {
        return Err(CertError::Invalid(
            "the local CA's store is made with its certificate; give the CA's subject".into(),
        ));
    }
    if !ks.path.starts_with('/') {
        return Err(CertError::Invalid(format!("{} is not an absolute path", ks.path)));
    }
    let password = ks
        .password
        .as_deref()
        .ok_or_else(|| CertError::Invalid("a new store needs a password".into()))?;
    let sealer = if stash { Some(ctx.sealer()?) } else { None };
    let mut store = CertStore::create_with(ctx.files, &ks.path, password, StoreKind::System, ctx.now, ctx.params)?;
    let mut notes = vec![Note::new(msgs::STORE_CREATED, &[ks.shown()])];
    if seed {
        let n = roots::seed(&mut store, ctx.now)?;
        store.save(ctx.files)?;
        notes.push(Note::new(msgs::IMPORTED, &[&format!("{n} root CAs"), ks.shown()]));
    }
    // A local CA that opens through its stash is trusted at once.
    if let Ok(ca_store) = ctx.open(&Keystore::new(super::LOCAL_CA_NAME, "")) {
        ca::trust(&ca_store, &mut store, ctx.now)?;
        store.save(ctx.files)?;
    }
    if let Some(s) = sealer {
        store.stash(ctx.files, password, s)?;
    }
    Ok(notes)
}

/// The local CA: its store, its certificate (also as PEM for clients),
/// and its stash. The `*SYSTEM` store, when it opens through its stash,
/// trusts the new CA.
pub fn create_local_ca(
    ctx: &Ctx<'_>,
    password: &str,
    subject: &Subject,
    alg: KeyAlgorithm,
    days: u32,
    stash: bool,
) -> Result<Vec<Note>> {
    let sealer = if stash { Some(ctx.sealer()?) } else { None };
    let info = ca::create(ctx.files, password, subject, alg, ctx.now, days, ctx.params)?;
    let ks = Keystore::new(super::LOCAL_CA_NAME, password);
    if let Some(s) = sealer {
        ctx.open(&ks)?.stash(ctx.files, password, s)?;
    }
    if let Ok(mut sys) = ctx.open(&Keystore::system()) {
        let ca_store = ctx.open(&ks)?;
        if ca::trust(&ca_store, &mut sys, ctx.now)? {
            sys.save(ctx.files)?;
        }
    }
    Ok(vec![
        Note::new(msgs::STORE_CREATED, &[ks.shown()]),
        Note::new(msgs::CREATED, &[info.common_name.as_deref().unwrap_or(ca::CA_LABEL), ks.shown()]),
    ])
}

/// Delete a store and its stash. Deleting `*SYSTEM` leaves every
/// application ID without a certificate; deleting the local CA's store
/// also removes its PEM copy.
pub fn delete(ctx: &Ctx<'_>, ks: &Keystore) -> Result<Vec<Note>> {
    if !ctx.files.exists(&ks.path)? {
        return Err(CertError::NotFound(ks.path.clone()));
    }
    // Only someone who can open it deletes it.
    ctx.open(ks)?;
    ctx.files.remove(&ks.path)?;
    ctx.files.remove(&stash_path(&ks.path))?;
    if ks.path == LOCAL_CA_STORE {
        ctx.files.remove(LOCAL_CA_CERT)?;
    }
    if ks.is_system() {
        let mut reg = AppRegistry::load(ctx.files)?;
        for a in &mut reg.apps {
            a.label = None;
            a.ca_trust = crate::apps::CaTrust::All;
        }
        reg.save(ctx.files)?;
    }
    Ok(vec![Note::new(msgs::STORE_DELETED, &[ks.shown()])])
}

/// Change a store's password, and its stash with it when it has one.
pub fn change_password(ctx: &Ctx<'_>, ks: &Keystore, new: &str) -> Result<Vec<Note>> {
    let mut store = ctx.open(ks)?;
    let old = match &ks.password {
        Some(p) => zeroize::Zeroizing::new(p.clone()),
        None => stash::read(ctx.files, &ks.path, ctx.sealer()?)?,
    };
    store.change_password(&old, new, ctx.now)?;
    store.save(ctx.files)?;
    if ctx.files.exists(&stash_path(&ks.path))? {
        store.stash(ctx.files, new, ctx.sealer()?)?;
    }
    Ok(vec![Note::new(msgs::CHANGED, &[ks.shown()])])
}

#[cfg(test)]
mod tests {
    use super::super::testing::*;
    use super::*;
    use crate::apps::AppRegistry;
    use crate::files::StoreFiles;
    use crate::paths::SYSTEM_STORE;

    #[test]
    fn creates_the_system_store_with_its_stash_and_roots() {
        let w = World::new();
        let ks = Keystore::new("*SYSTEM", "system1");
        let notes = create(&w.ctx(), &ks, true, true).unwrap();
        assert_eq!(notes[0].msgid, msgs::STORE_CREATED);
        assert_eq!(notes[1].msgid, msgs::IMPORTED);
        let s = w.ctx().open(&Keystore::system()).unwrap();
        assert!(s.trusted_cas().len() > 50);
        assert!(matches!(create(&w.ctx(), &ks, true, false), Err(CertError::Exists(_))));
        assert!(create(&w.ctx(), &Keystore::new("rel/x.kdb", "abcdef"), false, false).is_err());
        assert!(create(&w.ctx(), &Keystore::new("/x.kdb", "*STASH"), false, false).is_err());
    }

    #[test]
    fn the_local_ca_is_trusted_by_the_system_store() {
        let w = World::with_system();
        let notes = create_local_ca(&w.ctx(), "cacert1", &Subject::cn("QS400 CA"), KeyAlgorithm::EcdsaP256, 3650, true)
            .unwrap();
        assert_eq!(notes[1].data[0], "QS400 CA");
        assert!(w.files.read(LOCAL_CA_CERT).unwrap().is_some());
        let sys = w.ctx().open(&Keystore::system()).unwrap();
        assert_eq!(sys.trusted_ca_labels(), [ca::CA_LABEL]);
        // A store made later trusts it too.
        create(&w.ctx(), &Keystore::new("/home/a.kdb", "abcdef"), false, false).unwrap();
        let other = w.ctx().open(&Keystore::new("/home/a.kdb", "abcdef")).unwrap();
        assert_eq!(other.trusted_ca_labels().len(), 1);
    }

    #[test]
    fn deleting_the_system_store_clears_assignments() {
        let w = World::with_system();
        let mut reg = AppRegistry::load(&w.files).unwrap();
        reg.assign("QIBM_HTTP_SERVER_ADMIN", Some("X")).unwrap();
        reg.save(&w.files).unwrap();
        assert!(delete(&w.ctx(), &Keystore::new("*SYSTEM", "wrong1")).is_err());
        delete(&w.ctx(), &Keystore::system()).unwrap();
        assert!(w.files.read(SYSTEM_STORE).unwrap().is_none());
        assert!(w.files.read(&stash_path(SYSTEM_STORE)).unwrap().is_none());
        let reg = AppRegistry::load(&w.files).unwrap();
        assert_eq!(reg.get("QIBM_HTTP_SERVER_ADMIN").unwrap().label, None);
        assert!(matches!(delete(&w.ctx(), &Keystore::system()), Err(CertError::NotFound(_))));
    }

    #[test]
    fn a_new_password_rewrites_the_stash() {
        let w = World::with_system();
        change_password(&w.ctx(), &Keystore::system(), "system2").unwrap();
        assert!(w.ctx().open(&Keystore::new("*SYSTEM", "system2")).is_ok());
        assert!(w.ctx().open(&Keystore::system()).is_ok());
    }
}
