// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Audit hooks. The IFS reports what happened with the IFS-specific
//! fields (path, object type, file ID); the caller (the storage
//! service) decides with `QAUDCTL` and `QAUDLVL` whether to journal it
//! and turns it into `sec::audit` entries. Object reads and changes
//! (`ZR`, `ZC`) are only reported for objects whose auditing value asks
//! for them; the others are always reported.

use std::sync::Mutex;

/// An audit entry type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditKind {
    /// `ZR`: an audited object was read.
    Read,
    /// `ZC`: an audited object was changed.
    Change,
    /// `AF`: an authority check failed.
    AuthFail,
    /// `CO`: an object was created.
    Create,
    /// `DO`: an object was deleted.
    Delete,
    /// `OM`: an object was moved or renamed.
    Move,
    /// `CA`: an authority changed.
    AuthChange,
    /// `OW`: the owner changed.
    Owner,
    /// `PG`: the primary group changed.
    Group,
    /// `AD`: the auditing value changed.
    AuditChange,
}

impl AuditKind {
    /// The two-letter entry type.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Read => "ZR",
            Self::Change => "ZC",
            Self::AuthFail => "AF",
            Self::Create => "CO",
            Self::Delete => "DO",
            Self::Move => "OM",
            Self::AuthChange => "CA",
            Self::Owner => "OW",
            Self::Group => "PG",
            Self::AuditChange => "AD",
        }
    }
}

/// A file ID (`Qp0lFID_t`, 16 bytes): file system, file number and
/// generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FileId {
    /// File system.
    pub fs: u32,
    /// File number.
    pub ino: u64,
    /// Generation.
    pub generation: u32,
}

impl FileId {
    /// The 16 bytes.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[..4].copy_from_slice(&self.fs.to_be_bytes());
        b[4..12].copy_from_slice(&self.ino.to_be_bytes());
        b[12..].copy_from_slice(&self.generation.to_be_bytes());
        b
    }

    /// From the 16 bytes.
    #[must_use]
    pub fn from_bytes(b: &[u8; 16]) -> Self {
        Self {
            fs: u32::from_be_bytes(b[..4].try_into().expect("4")),
            ino: u64::from_be_bytes(b[4..12].try_into().expect("8")),
            generation: u32::from_be_bytes(b[12..].try_into().expect("4")),
        }
    }
}

/// One event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// The type.
    pub kind: AuditKind,
    /// The user.
    pub user: String,
    /// The object's path.
    pub path: String,
    /// Its type (`*STMF` ...).
    pub objtype: String,
    /// Its file ID.
    pub file_id: FileId,
    /// The operation.
    pub oper: &'static str,
    /// For `AF`: the violation type (`A` authority).
    pub violation: Option<char>,
    /// More: the new path of a move, the new owner, the authority given.
    pub detail: String,
}

/// Where events go.
pub trait Audit {
    /// One event.
    fn event(&self, e: AuditEvent);
}

/// Events dropped.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoAudit;

impl Audit for NoAudit {
    fn event(&self, _e: AuditEvent) {}
}

/// Events kept in memory (tests).
#[derive(Debug, Default)]
pub struct MemAudit(pub Mutex<Vec<AuditEvent>>);

impl MemAudit {
    /// The events so far, taken.
    pub fn take(&self) -> Vec<AuditEvent> {
        std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

impl Audit for MemAudit {
    fn event(&self, e: AuditEvent) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_ids() {
        let id = FileId {
            fs: 1,
            ino: 0x0102_0304_0506_0708,
            generation: 9,
        };
        assert_eq!(FileId::from_bytes(&id.to_bytes()), id);
        assert_eq!(AuditKind::Change.code(), "ZC");
    }
}
