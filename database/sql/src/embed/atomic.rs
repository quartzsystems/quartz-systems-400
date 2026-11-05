// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statements of an `ATOMIC` compound in a routine without
//! commitment control: they run under a definition of the routine's
//! own (its activation group's), so the compound can be undone, and
//! its work is committed when its savepoint is released.

use db::cmt::{CmtScope, LckLvl};

use super::{Session, Sqlca};
use crate::error::SqlError;

impl Session {
    /// The routine's own definition, started when it has none.
    pub(super) fn atomic_def(&mut self, group: &str, lcklvl: LckLvl) -> Option<Sqlca> {
        let now = self.ctx.clock();
        let mut st = self.store.store();
        let r = st.begin().map_err(SqlError::from).and_then(|mut txn| {
            match self.ctx.session.strcmtctl_in(&mut txn, lcklvl, None, now, CmtScope::ActGrp(group.to_string())) {
                Ok(()) | Err(db::DbError::CommitActive) => {}
                Err(e) => return Err(SqlError::from(e)),
            }
            txn.commit().map_err(SqlError::from)
        });
        r.err().map(|e| Sqlca::from_error(&e))
    }

    /// After `RELEASE SAVEPOINT` in such a compound: with no savepoint
    /// left, the outermost compound ended and its work is committed.
    pub(super) fn atomic_released(&mut self) -> Option<Sqlca> {
        let def = self.ctx.session.commit_def()?;
        if def.borrow().has_savepoints() {
            return None;
        }
        let now = self.ctx.now;
        let mut st = self.store.store();
        let r = st.begin().map_err(SqlError::from).and_then(|mut txn| {
            self.ctx.session.commit(&mut txn, now).map_err(SqlError::from)?;
            txn.commit().map_err(SqlError::from)
        });
        r.err().map(|e| Sqlca::from_error(&e))
    }
}
