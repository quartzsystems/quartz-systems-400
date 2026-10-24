// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What each IFS operation needs, as IBM documents it for root,
//! `/QOpenSys` and UDFS: on the directory holding the object (beyond
//! `*X` on every directory of the path, which resolution checks) and on
//! the object itself. `docs/IFS.md` has the table.

use sec::auth::bits::Auth;
use sec::profile::spcaut::SpecialAuth;
use store::obj::header::Authority as A;

use super::data::{R, RW, W, WX, X};

/// An operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    /// Pass through a directory while resolving a path.
    Search,
    /// Open for reading.
    OpenRead,
    /// Open for writing (or truncating).
    OpenWrite,
    /// Open for reading and writing.
    OpenRdWr,
    /// Run a program or script.
    Execute,
    /// Create a stream file, FIFO, socket or symbolic link.
    Create,
    /// Make a directory.
    Mkdir,
    /// Remove a link to a non-directory.
    Unlink,
    /// Remove a directory.
    Rmdir,
    /// Rename or move: the object.
    Rename,
    /// Rename onto an existing object, which is replaced.
    RenameReplace,
    /// A new hard link to the object.
    Link,
    /// Read a directory's entries.
    Readdir,
    /// Read a symbolic link's text.
    Readlink,
    /// Read attributes (`stat`, `Qp0lGetAttr`).
    GetAttr,
    /// Change mode bits (`chmod`).
    Chmod,
    /// Change the owner (`chown`, `CHGOWN`).
    Chown,
    /// Change the primary group (`chgrp`, `CHGPGP`).
    Chgrp,
    /// Change authorities (`CHGAUT`).
    ChgAut,
    /// Change PC attributes, the CCSID or the text (`CHGATR`).
    SetAttr,
    /// Change the storage, save and scan attributes (`CHGATR`).
    SetAttrMgt,
    /// Change the auditing value (`CHGAUD`, `CHGATR *AUD`).
    SetAudit,
    /// Set the times to now (`utime` with no times).
    UtimeNow,
    /// Set the times to given values.
    UtimeSet,
    /// Truncate by path.
    Truncate,
    /// Check out (`CHKOUT`).
    CheckOut,
    /// Start or end journaling (`STRJRN`, `ENDJRN`).
    Journal,
    /// Read for a copy (`CPY` source).
    CopyFrom,
    /// Save (`SAV`).
    Save,
}

/// What the object needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjNeed {
    /// Nothing beyond finding it.
    None,
    /// These authorities.
    Bits(Auth),
    /// Be the owner or have `*ALLOBJ`.
    OwnerOrAllObj,
    /// Be the owner, have `*ALLOBJ`, or have these authorities.
    OwnerOr(Auth),
    /// A special authority.
    Special(SpecialAuth),
    /// These authorities, or a special authority.
    BitsOrSpecial(Auth, SpecialAuth),
}

/// One row of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// Needed on the directory that holds the object.
    pub dir: Option<Auth>,
    /// Needed on the object.
    pub obj: ObjNeed,
}

const fn rule(dir: Option<Auth>, obj: ObjNeed) -> Rule {
    Rule { dir, obj }
}

/// `*OBJEXIST`.
pub const OBJEXIST: Auth = Auth(A::OBJEXIST);
/// `*OBJMGT`.
pub const OBJMGT: Auth = Auth(A::OBJMGT);
/// `*OBJALTER`.
pub const OBJALTER: Auth = Auth(A::OBJALTER);

/// The rule for an operation.
#[must_use]
pub fn rule_of(op: Op) -> Rule {
    use ObjNeed as O;
    match op {
        Op::Search | Op::Execute => rule(None, O::Bits(X)),
        Op::OpenRead | Op::Readdir | Op::CopyFrom => rule(None, O::Bits(R)),
        Op::OpenWrite | Op::Truncate | Op::CheckOut => rule(None, O::Bits(W)),
        Op::OpenRdWr => rule(None, O::Bits(RW)),
        Op::Create | Op::Mkdir => rule(Some(WX), O::None),
        Op::Unlink | Op::Rmdir | Op::RenameReplace | Op::Link => rule(Some(WX), O::Bits(OBJEXIST)),
        Op::Rename => rule(Some(WX), O::Bits(OBJMGT)),
        Op::Readlink | Op::GetAttr => rule(None, O::None),
        Op::Chmod | Op::Chown | Op::Chgrp | Op::UtimeSet => rule(None, O::OwnerOrAllObj),
        Op::ChgAut => rule(None, O::OwnerOr(OBJMGT)),
        Op::SetAttr | Op::UtimeNow => rule(None, O::OwnerOr(W)),
        Op::SetAttrMgt => rule(None, O::OwnerOr(OBJALTER)),
        Op::SetAudit => rule(None, O::Special(SpecialAuth::AUDIT)),
        Op::Journal => rule(None, O::Bits(OBJMGT)),
        Op::Save => rule(None, O::BitsOrSpecial(OBJEXIST, SpecialAuth::SAVSYS)),
    }
}

/// Whether an operation changes the object (for `ZC` against `ZR`).
#[must_use]
pub fn is_change(op: Op) -> bool {
    !matches!(
        op,
        Op::Search
            | Op::OpenRead
            | Op::Execute
            | Op::Readdir
            | Op::Readlink
            | Op::GetAttr
            | Op::CopyFrom
            | Op::Save
    )
}

/// The operation's name in audit entries (24 characters at most).
#[must_use]
pub fn name(op: Op) -> &'static str {
    match op {
        Op::Search => "SEARCH",
        Op::OpenRead => "OPEN READ",
        Op::OpenWrite => "OPEN WRITE",
        Op::OpenRdWr => "OPEN READ WRITE",
        Op::Execute => "EXECUTE",
        Op::Create => "CREATE",
        Op::Mkdir => "MKDIR",
        Op::Unlink => "UNLINK",
        Op::Rmdir => "RMDIR",
        Op::Rename => "RENAME",
        Op::RenameReplace => "RENAME REPLACE",
        Op::Link => "LINK",
        Op::Readdir => "READDIR",
        Op::Readlink => "READLINK",
        Op::GetAttr => "GET ATTRIBUTES",
        Op::Chmod => "CHMOD",
        Op::Chown => "CHOWN",
        Op::Chgrp => "CHGRP",
        Op::ChgAut => "CHGAUT",
        Op::SetAttr => "SET ATTRIBUTES",
        Op::SetAttrMgt => "SET STORAGE ATTRIBUTES",
        Op::SetAudit => "SET AUDITING",
        Op::UtimeNow => "UTIME",
        Op::UtimeSet => "UTIME TIMES",
        Op::Truncate => "TRUNCATE",
        Op::CheckOut => "CHECK OUT",
        Op::Journal => "JOURNAL",
        Op::CopyFrom => "COPY",
        Op::Save => "SAVE",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The IBM table, row by row.
    #[test]
    fn requirement_table() {
        use ObjNeed as O;
        let rows: &[(Op, Option<Auth>, ObjNeed)] = &[
            (Op::Search, None, O::Bits(X)),
            (Op::OpenRead, None, O::Bits(R)),
            (Op::OpenWrite, None, O::Bits(W)),
            (Op::OpenRdWr, None, O::Bits(RW)),
            (Op::Execute, None, O::Bits(X)),
            (Op::Create, Some(WX), O::None),
            (Op::Mkdir, Some(WX), O::None),
            (Op::Unlink, Some(WX), O::Bits(OBJEXIST)),
            (Op::Rmdir, Some(WX), O::Bits(OBJEXIST)),
            (Op::Rename, Some(WX), O::Bits(OBJMGT)),
            (Op::RenameReplace, Some(WX), O::Bits(OBJEXIST)),
            (Op::Link, Some(WX), O::Bits(OBJEXIST)),
            (Op::Readdir, None, O::Bits(R)),
            (Op::Readlink, None, O::None),
            (Op::GetAttr, None, O::None),
            (Op::Chmod, None, O::OwnerOrAllObj),
            (Op::Chown, None, O::OwnerOrAllObj),
            (Op::Chgrp, None, O::OwnerOrAllObj),
            (Op::ChgAut, None, O::OwnerOr(OBJMGT)),
            (Op::SetAttr, None, O::OwnerOr(W)),
            (Op::SetAttrMgt, None, O::OwnerOr(OBJALTER)),
            (Op::SetAudit, None, O::Special(SpecialAuth::AUDIT)),
            (Op::UtimeNow, None, O::OwnerOr(W)),
            (Op::UtimeSet, None, O::OwnerOrAllObj),
            (Op::Truncate, None, O::Bits(W)),
            (Op::CheckOut, None, O::Bits(W)),
            (Op::Journal, None, O::Bits(OBJMGT)),
            (Op::CopyFrom, None, O::Bits(R)),
            (
                Op::Save,
                None,
                O::BitsOrSpecial(OBJEXIST, SpecialAuth::SAVSYS),
            ),
        ];
        for (op, dir, obj) in rows {
            assert_eq!(
                rule_of(*op),
                Rule {
                    dir: *dir,
                    obj: *obj
                },
                "{op:?}"
            );
            assert!(name(*op).len() <= 24);
        }
        assert!(is_change(Op::Unlink));
        assert!(!is_change(Op::OpenRead));
    }
}
