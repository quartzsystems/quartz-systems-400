// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the security layer decides for a save or restore. The engine
//! knows objects; who may save them, which profiles exist and what goes
//! to the audit journal are the integrator's (`slic` over `sec`).

use savfmt::ObjectDescriptor;
use store::obj::{Authority, ObjectHeader, ObjectId, auth};
use store::txn::Txn;

/// The engine's questions to the security layer.
pub trait Hooks {
    /// May the requester save this object? `Err` is the message.
    fn may_save(&self, _txn: &mut Txn<'_>, _h: &ObjectHeader) -> Result<(), &'static str> {
        Ok(())
    }

    /// May the requester restore this saved object into library `lib`,
    /// over `existing` when there is one? `Err` is the message.
    fn may_restore(
        &self,
        _txn: &mut Txn<'_>,
        _d: &ObjectDescriptor,
        _lib: ObjectId,
        _existing: Option<&ObjectHeader>,
    ) -> Result<(), &'static str> {
        Ok(())
    }

    /// Give `profile` a private authority to `id` (the security layer
    /// also indexes it under the profile).
    fn set_private(
        &self,
        txn: &mut Txn<'_>,
        id: ObjectId,
        profile: [u8; 10],
        bits: u32,
    ) -> store::Result<()> {
        let (page, mut h) = auth::load(txn, id)?;
        auth::private_set(txn, page, &mut h, profile, Some(Authority(bits)))?;
        Ok(())
    }

    /// Secure `id` with authorization list `autl`.
    fn secure(&self, _txn: &mut Txn<'_>, _autl: &str, _id: ObjectId) -> store::Result<()> {
        Ok(())
    }

    /// Whether `signature` by the key `key_id` names is good for
    /// `digest`; `None` when the key is not one the system knows.
    fn verify(&self, _key_id: [u8; 8], _digest: &[u8; 32], _signature: &[u8; 64]) -> Option<bool> {
        None
    }

    /// Whether `key_id` is a key the system trusts for system objects.
    fn system_key(&self, _key_id: [u8; 8]) -> bool {
        false
    }

    /// Whether a user profile exists.
    fn profile_exists(&self, _txn: &mut Txn<'_>, _name: &str) -> bool {
        true
    }

    /// Whether an authorization list exists.
    fn autl_exists(&self, _txn: &mut Txn<'_>, _name: &str) -> bool {
        true
    }

    /// The object was restored (`replaced`: over an existing one). The
    /// owner index and the audit entry are the layer's.
    fn restored(&self, _txn: &mut Txn<'_>, _h: &ObjectHeader, _replaced: bool) {}

    /// The object was saved.
    fn saved(&self, _txn: &mut Txn<'_>, _h: &ObjectHeader) {}
}

/// Hooks that allow everything: tests, and the IPL's own restores.
pub struct AllowAll;

impl Hooks for AllowAll {}
