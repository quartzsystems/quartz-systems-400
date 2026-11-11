// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The store's watcher (`store::obj::watch`) for the storage service:
//! libraries, files and members changed through the database or the
//! object commands, not the IFS, break SMB leases and feed change notify
//! on their `/QSYS.LIB` nodes (`docs/NETSERVER.md` section 6,
//! `ifs::lease::qsys`). Changes the IFS makes through [`StoreQsys`]
//! itself are left out: the IFS operation has broken and noted them.
//! With no lease held and no directory watched it costs one check.
//!
//! [`StoreQsys`]: crate::StoreQsys

use std::cell::Cell;
use std::sync::Arc;

use ifs::Errno;
use ifs::lease::Leases;
use ifs::lease::qsys::{QsysChange, changed};
use ifs::ops::Owner;
use ifs::vfs::qsys::QsysNode;
use qsname::Name;
use store::obj::watch::{Change, Watcher};
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;
use store::{Result, StoreError};

thread_local! {
    /// An IFS call through the view is in progress on this thread.
    static IN_VIEW: Cell<bool> = const { Cell::new(false) };
}

/// Run `f`, the view's store call: what it changes is the IFS's to tell.
pub(crate) fn in_view<T>(f: impl FnOnce() -> T) -> T {
    let before = IN_VIEW.with(|v| v.replace(true));
    let r = f();
    IN_VIEW.with(|v| v.set(before));
    r
}

/// The watcher over one IFS's leases and watches.
pub struct Notice {
    leases: Arc<Leases>,
}

impl Notice {
    /// Over `leases` (`Ifs::leases_shared`).
    #[must_use]
    pub fn new(leases: Arc<Leases>) -> Self {
        Self { leases }
    }
}

/// A library's name as `/QSYS.LIB` writes it.
fn lib_name(txn: &mut Txn<'_>, lib: ObjectId) -> Option<String> {
    Some(ops::read_header(txn, lib).ok()?.1.name.to_string())
}

/// The node of a library, file or member, with `name` in place of its
/// own and `library` in place of its library when given.
fn node_of(
    txn: &mut Txn<'_>,
    id: ObjectId,
    name: Option<Name>,
    library: Option<ObjectId>,
) -> Option<QsysNode> {
    let (_, h) = ops::read_header(txn, id).ok()?;
    let own = name.unwrap_or(h.name).to_string();
    match id.type_code {
        // QSYS is `/QSYS.LIB` itself.
        types::LIB if h.library != id => Some(QsysNode::Lib(own)),
        types::FILE => Some(QsysNode::Obj(
            lib_name(txn, library.unwrap_or(h.library))?,
            own,
            "*FILE".into(),
        )),
        types::MBR => {
            let (_, f) = ops::read_header(txn, h.library).ok()?;
            Some(QsysNode::Mbr(
                lib_name(txn, f.library)?,
                f.name.to_string(),
                own,
            ))
        }
        _ => None,
    }
}

/// A file's members as nodes.
fn members_of(txn: &mut Txn<'_>, file: ObjectId, node: &QsysNode) -> Vec<QsysNode> {
    let QsysNode::Obj(lib, f, _) = node else {
        return Vec::new();
    };
    db::file::describe(txn, file).map_or_else(
        |_| Vec::new(),
        |d| {
            d.members
                .iter()
                .map(|m| QsysNode::Mbr(lib.clone(), f.clone(), m.name.to_string()))
                .collect()
        },
    )
}

impl Watcher for Notice {
    fn active(&self) -> bool {
        !IN_VIEW.with(Cell::get) && !self.leases.idle()
    }

    fn changed(&self, txn: &mut Txn<'_>, job: JobHandle, id: ObjectId, what: Change) -> Result<()> {
        let Some(node) = node_of(txn, id, None, None) else {
            return Ok(());
        };
        let c = match what {
            Change::Opening => QsysChange::Opening,
            Change::Data => QsysChange::Data,
            Change::Added => QsysChange::Added,
            Change::Removing => QsysChange::Removing {
                members: members_of(txn, id, &node),
            },
            Change::Renamed { from } => match node_of(txn, id, Some(from), None) {
                Some(from) => QsysChange::Renamed { from },
                None => return Ok(()),
            },
            Change::Moved { from } => match node_of(txn, id, None, Some(from)) {
                Some(from) => QsysChange::Renamed { from },
                None => return Ok(()),
            },
        };
        match changed(&self.leases, Owner(job.0), &node, &c) {
            Err(Errno::EAGAIN) => Err(StoreError::Deferred),
            _ => Ok(()),
        }
    }
}
