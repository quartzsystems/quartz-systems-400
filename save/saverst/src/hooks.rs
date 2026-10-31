// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the security layer decides for a save or restore. The engine
//! knows objects; who may save them, which profiles exist and what goes
//! to the audit journal are the integrator's (`slic` over `sec`).

use savfmt::ObjectDescriptor;
use store::obj::ObjectHeader;
use store::txn::Txn;

/// The engine's questions to the security layer.
pub trait Hooks {
    /// May the requester save this object? `Err` is the message.
    fn may_save(&self, _txn: &mut Txn<'_>, _h: &ObjectHeader) -> Result<(), &'static str> {
        Ok(())
    }

    /// May the requester restore this saved object, over `existing`
    /// when there is one? `Err` is the message.
    fn may_restore(
        &self,
        _txn: &mut Txn<'_>,
        _d: &ObjectDescriptor,
        _existing: Option<&ObjectHeader>,
    ) -> Result<(), &'static str> {
        Ok(())
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
