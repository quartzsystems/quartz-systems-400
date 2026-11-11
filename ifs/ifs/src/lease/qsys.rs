// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/QSYS.LIB` changed outside the IFS (`docs/NETSERVER.md` section 6):
//! record I/O, SQL, `CLRPFM`, `ADDPFM`, `RMVM`, `DLTF`, library
//! operations. The database reports them by object; the service names
//! the node and calls [`changed`], which breaks leases and feeds watches
//! as the IFS operation with the same effect does. Keys and paths are
//! the IFS's own ([`crate::resolve::qsys_key`], [`path_of`]).

use super::Leases;
use super::defer;
use super::notify::ChangeKind;
use super::state::Trigger;
use crate::errno::Result;
use crate::lock::Owner;
use crate::resolve::qsys_key;
use crate::vfs::qsys::{QsysNode, path_of};

/// What happened to a library, file or member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QsysChange {
    /// A member is about to be opened for change: write caching is
    /// broken, and the open waits for it (as an IFS open does) when its
    /// caller allows a deferral.
    Opening,
    /// A member's records changed: as an IFS write (every lease broken,
    /// never waiting: the open waited) and `Modified` to its file's
    /// watches.
    Data,
    /// The node was made.
    Added,
    /// The node is about to go, with these members (of a file): handle
    /// caching broken on each member, as an IFS unlink.
    Removing {
        /// A file's members.
        members: Vec<QsysNode>,
    },
    /// The node was renamed or moved from `from`.
    Renamed {
        /// Where it was.
        from: QsysNode,
    },
}

/// `by` did `what` to `node`.
///
/// # Errors
///
/// `EAGAIN` for an [`QsysChange::Opening`] that has to wait, when its
/// caller allowed a deferral ([`defer::deferring`]).
pub fn changed(leases: &Leases, by: Owner, node: &QsysNode, what: &QsysChange) -> Result<()> {
    let member = matches!(node, QsysNode::Mbr(..));
    let path = path_of(node);
    match what {
        QsysChange::Opening => {
            if member {
                leases.gate(by, qsys_key(node), Trigger::Open { overwrite: false })?;
            }
        }
        QsysChange::Data => {
            if member {
                defer::whole(|| leases.gate(by, qsys_key(node), Trigger::Write))?;
                leases.note(by, &path, &ChangeKind::Modified, false);
            }
        }
        QsysChange::Added => leases.note(by, &path, &ChangeKind::Added, !member),
        QsysChange::Removing { members } => {
            let gone = std::iter::once(node)
                .chain(members)
                .filter(|n| matches!(n, QsysNode::Mbr(..)));
            for m in gone {
                defer::whole(|| leases.gate(by, qsys_key(m), Trigger::Handle))?;
            }
            if member {
                leases.note(by, &path, &ChangeKind::Removed, false);
            } else {
                leases.note_rmdir(by, &path);
            }
        }
        QsysChange::Renamed { from } => {
            if member {
                defer::whole(|| leases.gate(by, qsys_key(from), Trigger::Handle))?;
            }
            leases.note_rename(by, &path_of(from), &path, !member);
        }
    }
    Ok(())
}
