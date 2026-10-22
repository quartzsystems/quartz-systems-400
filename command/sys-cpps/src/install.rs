// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Installing the command set into `QSYS`: every definition compiled
//! and created as a `*CMD` with its processing program; a command
//! whose stored source differs from the shipped one is replaced, so an
//! upgraded system takes new definitions at its first IPL.

use cmd::def::Qual;
use cmd::payload::CmdObject;
use cmd::store::{Ctx, crtcmd, load};
use qsname::Name;
use store::Txn;
use store::obj::library::Library;
use store::obj::{ObjectId, types};

use crate::inventory;

/// What installing did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Installed {
    /// Commands created.
    pub created: Vec<String>,
    /// Commands replaced with a newer definition.
    pub replaced: Vec<String>,
    /// Definitions that did not compile, with their first diagnostic.
    pub failed: Vec<(String, String)>,
}

/// Compile one shipped definition into its object.
pub fn object(name: &str, now: u64) -> Result<CmdObject, Vec<cmd::Diag>> {
    let source = crate::defs::source(name).ok_or_default()?;
    let mut def = cmd::source::compile(name, source)?;
    let e = inventory::entry(name).expect("every definition is in the inventory");
    def.cpp = Qual::new(&e.cpp, "QSYS");
    def.alwlmtusr = e.alwlmtusr;
    if def.text.is_empty() {
        def.text.clone_from(&def.prompt);
    }
    Ok(CmdObject::new(def, source.to_string(), now))
}

trait OkOrDefault<T> {
    fn ok_or_default(self) -> Result<T, Vec<cmd::Diag>>;
}

impl<T> OkOrDefault<T> for Option<T> {
    fn ok_or_default(self) -> Result<T, Vec<cmd::Diag>> {
        self.ok_or_else(|| vec![cmd::Diag::new("CPD0030")])
    }
}

/// Install every command into `QSYS`.
pub fn install(txn: &mut Txn<'_>, ctx: &Ctx, qsys: ObjectId) -> cmd::Result<Installed> {
    let mut done = Installed::default();
    let lib = Library::load(txn, qsys)?;
    for (name, source) in crate::defs::DEFS {
        let n = Name::parse(name).map_err(|e| cmd::CmdError::detail("CPD0078", e.to_string()))?;
        let existing = lib.lookup(txn, types::CMD, n)?;
        if let Some(id) = existing {
            let same = load(txn, id).is_ok_and(|o| o.source == *source);
            if same {
                continue;
            }
        }
        let o = match object(name, ctx.now) {
            Ok(o) => o,
            Err(diags) => {
                let first = diags
                    .first()
                    .map(|d| format!("{} {}", d.msgid, d.detail))
                    .unwrap_or_default();
                done.failed.push(((*name).to_string(), first));
                continue;
            }
        };
        crtcmd(txn, ctx, qsys, n, &o, existing.is_some(), true)?;
        if existing.is_some() {
            done.replaced.push((*name).to_string());
        } else {
            done.created.push((*name).to_string());
        }
    }
    Ok(done)
}
