// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL packages (`*SQLPKG`): statements kept prepared for a client, made
//! by the database host server's extended dynamic support and by a DRDA
//! requester's bind (`docs/DB_FORMAT.md` §9).

pub mod format;

use qsname::Name;
use store::obj::library::Library;
use store::obj::ops::{self, CreateParams};
use store::obj::space::Space;
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

pub use format::{PkgAttr, PkgKind, PkgStmt};

use crate::error::Result;

/// The `*SQLPKG` type code.
pub const SQLPKG: u16 = 0x001E;

/// A package: its attributes and statements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Package {
    /// The environment its statements were prepared in.
    pub attr: PkgAttr,
    /// The statements, in the order they were added.
    pub stmts: Vec<PkgStmt>,
}

impl Package {
    /// The statement named `name`.
    #[must_use]
    pub fn stmt(&self, name: &str) -> Option<&PkgStmt> {
        self.stmts.iter().find(|s| s.name.eq_ignore_ascii_case(name.trim()))
    }

    /// The statement with this text.
    #[must_use]
    pub fn by_text(&self, text: &str) -> Option<&PkgStmt> {
        self.stmts.iter().find(|s| s.text == text)
    }

    /// The name the next statement takes: `QZ`, the token's low 52 bits
    /// in 13 hexadecimal digits and a 3-digit sequence, the form the
    /// primary's packages show (`QZB027CC62A9389001`).
    #[must_use]
    pub fn next_name(&self) -> String {
        format!("QZ{:013X}{:03}", self.attr.token & 0x000F_FFFF_FFFF_FFFF, (self.stmts.len() + 1) % 1000)
    }
}

/// Package `name` in `lib`, if there is one: `*EXECUTE` to the library
/// first, as naming any object in it needs.
pub fn find(txn: &mut Txn<'_>, lib: ObjectId, name: Name) -> Result<Option<ObjectId>> {
    crate::auth::check(txn, "lib.search", lib)?;
    Ok(Library::load(txn, lib)?.lookup(txn, SQLPKG, name)?)
}

/// Make package `name` in `lib` with no statements, owned by `owner`.
pub fn create(txn: &mut Txn<'_>, lib: ObjectId, name: Name, attr: &PkgAttr, owner: &str, text: &str, now: u64) -> Result<ObjectId> {
    let id = ops::create(
        txn,
        &CreateParams {
            library: lib,
            name,
            type_code: SQLPKG,
            attribute: "PACKAGE".into(),
            text: text.into(),
            owner: owner.into(),
            system: "QS400".into(),
            now,
            by_system: false,
        },
    )?;
    save(txn, id, &Package { attr: attr.clone(), stmts: Vec::new() })?;
    Ok(id)
}

/// The package `id`.
pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<Package> {
    let (page, _) = ops::read_header(txn, id)?;
    let (attr, count, len) = PkgAttr::decode(&ops::read_attr(txn, page)?)?;
    let bytes = if len == 0 {
        Vec::new()
    } else {
        let (space, _) = Space::load(txn, id)?;
        space.read(txn, 0, usize::try_from(len).expect("fits"))?
    };
    Ok(Package { attr, stmts: PkgStmt::decode_all(&bytes, count)? })
}

/// Write `pkg` as package `id`'s contents.
pub fn save(txn: &mut Txn<'_>, id: ObjectId, pkg: &Package) -> Result<()> {
    let body: Vec<u8> = pkg.stmts.iter().flat_map(PkgStmt::encode).collect();
    drop(Space::truncate(txn, id, 0)?);
    if !body.is_empty() {
        let mut space = Space::ensure(txn, id, body.len() as u64, 0)?;
        space.write_relocating(txn, 0, &body)?;
    }
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &pkg.attr.encode(pkg.stmts.len(), body.len() as u64))?;
    Ok(())
}

/// Delete package `id`.
pub fn delete(txn: &mut Txn<'_>, job: JobHandle, id: ObjectId) -> Result<()> {
    ops::delete(txn, job, id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn statements_round_trip() {
        let s = PkgStmt {
            name: "QZ0000000000001001".into(),
            stmt_type: 2,
            needs_collection: true,
            section: 0,
            text: "SELECT CAST(? AS INTEGER) + 1 AS N FROM SYSIBM.SYSDUMMY1".into(),
            cols: vec![1, 2, 3],
            markers: vec![4, 5],
        };
        let t = PkgStmt { name: "S2".into(), section: 2, text: "DECLARE C2 CURSOR FOR S2".into(), ..PkgStmt::default() };
        let mut b = s.encode();
        b.extend(t.encode());
        assert_eq!(PkgStmt::decode_all(&b, 2).unwrap(), vec![s, t]);
        let a = PkgAttr { kind: PkgKind::Bound, sql_naming: true, commit: 2, token: 77, default_collection: "QSTEST".into(), ..PkgAttr::default() };
        let (back, n, len) = PkgAttr::decode(&a.encode(2, b.len() as u64)).unwrap();
        assert_eq!((back, n, len), (a, 2, b.len() as u64));
    }

    #[test]
    fn names_follow_the_primary() {
        let p = Package { attr: PkgAttr { token: 0x000B_027C_C62A_9389_u64, ..PkgAttr::default() }, stmts: Vec::new() };
        assert_eq!(p.next_name(), "QZB027CC62A9389001");
    }
}
