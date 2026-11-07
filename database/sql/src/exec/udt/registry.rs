// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The distinct types a statement can name, worked out before it runs:
//! result types are computed without the store (`exec::restype`), so the
//! types are listed first ([`enter`]) and looked up by name as the
//! statement is typed and run. The list is kept while no library or type
//! has come or gone (`store::obj::library::types_epoch`).

use std::cell::RefCell;
use std::rc::Rc;

use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use crate::ast::TypeSpec;
use crate::exec::SqlContext;

/// One distinct type.
#[derive(Clone, Debug)]
pub struct Distinct {
    /// Its `*SQLUDT`.
    pub id: ObjectId,
    /// Its library.
    pub lib: ObjectId,
    /// Its library's system name.
    pub schema: Name,
    /// Its SQL name.
    pub name: String,
    /// The source type.
    pub source: TypeSpec,
}

impl Distinct {
    /// `SCHEMA.NAME`, as a column of the type describes.
    #[must_use]
    pub fn qualified(&self) -> String {
        format!("{}.{}", self.schema.to_string().trim_end(), self.name)
    }

    /// The source type marked as this type.
    #[must_use]
    pub fn spec(&self) -> TypeSpec {
        TypeSpec {
            udt: Some(self.qualified()),
            ..self.source.clone()
        }
    }
}

#[derive(Default)]
struct State {
    /// The store and epoch the list is of.
    key: Option<(usize, u64)>,
    types: Rc<Vec<Distinct>>,
    /// Where an unqualified name is looked for, in order.
    path: Vec<ObjectId>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Make the types of `txn`'s store the ones the statement about to run
/// sees, and its path the context's.
pub fn enter(txn: &mut Txn<'_>, ctx: &SqlContext) {
    let store = std::ptr::from_ref(txn.store()) as usize;
    let epoch = store::obj::library::types_epoch();
    if STATE.with(|s| s.borrow().key != Some((store, epoch))) {
        let types = Rc::new(list(txn, ctx.qsys));
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.key = Some((store, epoch));
            s.types = types;
        });
    }
    // No types, no path to work out (most statements).
    let path = if STATE.with(|s| s.borrow().types.is_empty()) {
        Vec::new()
    } else {
        crate::exec::routine::find::path(txn, ctx)
    };
    STATE.with(|s| s.borrow_mut().path = path);
}

/// Every distinct type of the store (array types left out).
fn list(txn: &mut Txn<'_>, qsys: ObjectId) -> Vec<Distinct> {
    let Ok(libs) = ops::list(txn, qsys, Some(types::LIB)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, lib) in libs {
        let Ok(udts) = ops::list(txn, lib, Some(types::SQLUDT)) else {
            continue;
        };
        for (sys, id) in udts {
            let Ok(Some(info)) = db::sqlinf::load(txn, id) else {
                continue;
            };
            let Some(Ok(u)) = info
                .first(db::sqlinf::tag::TYPE)
                .map(db::sqlinf::var::Udt::decode)
            else {
                continue;
            };
            if u.metatype != b'T' {
                continue;
            }
            let Ok(source) = crate::pl::types::parse_type(&u.source) else {
                continue;
            };
            let name = info
                .name()
                .unwrap_or_else(|| sys.name.to_string().trim_end().to_string());
            out.push(Distinct {
                id,
                lib,
                schema: k.name,
                name,
                source,
            });
        }
    }
    out
}

/// The type `name` (`SCHEMA.NAME` or `NAME`, as written) means: its
/// schema's, else the first in the path (`*SQL`) or the library list.
#[must_use]
pub fn find(name: &str) -> Option<Distinct> {
    let (schema, base) = match name.split_once('.') {
        Some((s, n)) => (Some(s), n),
        None => (None, name),
    };
    STATE.with(|s| {
        let s = s.borrow();
        let named = |d: &&Distinct| d.name == base;
        if let Some(sc) = schema {
            return s
                .types
                .iter()
                .filter(named)
                .find(|d| d.schema.to_string().trim_end() == sc)
                .cloned();
        }
        s.path
            .iter()
            .find_map(|l| s.types.iter().filter(named).find(|d| d.lib == *l).cloned())
    })
}

/// The type a `SCHEMA.NAME` a column carries names.
#[must_use]
pub fn by_qualified(q: &str) -> Option<Distinct> {
    STATE.with(|s| {
        s.borrow()
            .types
            .iter()
            .find(|d| d.qualified() == q)
            .cloned()
    })
}

/// Whether library `lib` is searched for an unqualified function name.
#[must_use]
pub fn in_path(lib: ObjectId) -> bool {
    STATE.with(|s| s.borrow().path.contains(&lib))
}

/// Whether the store has any distinct type.
#[must_use]
pub fn any() -> bool {
    STATE.with(|s| !s.borrow().types.is_empty())
}

/// Whether a distinct type of schema `schema` has `function` (the
/// source type's name) generated for it.
#[must_use]
pub fn with_source_in(schema: &str, function: &str) -> bool {
    STATE.with(|s| {
        s.borrow().types.iter().any(|d| {
            d.schema.to_string().trim_end() == schema
                && super::typing::source_function(&d.source.data_type) == function
        })
    })
}
