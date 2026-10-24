// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The symbol table: every name a module declares, with its type, its
//! IR place, its scope and the references the cross-reference lists.
//! Global names live in scope 0; each subprocedure has its own scope,
//! searched first.

use std::collections::HashMap;

use rpgfront::ast::Expr;

use crate::types::Ty;

/// A symbol's index.
pub type SymId = usize;

/// What a constant holds.
#[derive(Clone, Debug, PartialEq)]
pub enum ConstVal {
    /// A literal or figurative constant, as written.
    Expr(Expr),
}

/// What a symbol is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A standalone field (or an array or table).
    Field,
    /// A named constant.
    Const(ConstVal),
    /// A data structure: its index in [`Table::ds`].
    Ds(usize),
    /// A subfield of a data structure.
    Subfield,
    /// A parameter of the procedure being compiled.
    Param,
    /// A prototype: its index in [`Table::protos`].
    Proto(usize),
    /// A file: its index in the module's files.
    File(usize),
    /// A record format of a file.
    Format(usize),
    /// A subroutine.
    Subr,
    /// A `TAG` label.
    Label,
    /// A `KLIST`.
    Klist,
    /// A `PLIST`.
    Plist,
}

/// How a field's storage is held.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Storage {
    /// Static: global, or a procedure's `STATIC`.
    #[default]
    Static,
    /// A procedure's automatic storage.
    Auto,
    /// `BASED(ptr)`: the pointer's symbol.
    Based(SymId),
    /// Inside a data structure (the parent's storage).
    InDs,
    /// A parameter.
    Param,
    /// Exported data.
    Export,
    /// Imported data.
    Import,
}

/// A symbol.
#[derive(Clone, Debug, Default)]
pub struct Sym {
    /// Upper-case name.
    pub name: String,
    /// What it is.
    pub kind: Option<Kind>,
    /// Its type.
    pub ty: Ty,
    /// Elements, 0 for a scalar.
    pub dim: u32,
    /// The IR place (`$NAME`, `$DS.F`).
    pub ir: String,
    /// The scope it lives in: 0 global, else the procedure's.
    pub scope: usize,
    /// Its storage.
    pub storage: Storage,
    /// The parent data structure symbol, for a subfield.
    pub parent: Option<SymId>,
    /// The offset in the parent, from 0.
    pub offset: u32,
    /// The `INZ` value, when given (`INZ` alone is `Some(None)`).
    pub inz: Option<Option<Expr>>,
    /// Where it is defined (listing line).
    pub def_line: u32,
    /// It is a `CONST` or `VALUE` parameter.
    pub readonly: bool,
    /// A table (`TABxxx`), searched by `LOOKUP`.
    pub table: bool,
    /// Compile-time data name for `CTDATA` arrays.
    pub ctdata: bool,
    /// `PERRCD` for compile-time arrays.
    pub perrcd: u32,
    /// Declared `TEMPLATE`: no storage.
    pub template: bool,
    /// The `EXPORT` or `IMPORT` external name.
    pub external: String,
    /// Implicitly defined (a fixed-form result field with a length,
    /// an external field).
    pub implicit: bool,
    /// `ALT(name)`: the array this one alternates with.
    pub alt: String,
    /// `ASCEND` 1, `DESCEND` 2, else 0.
    pub seq: u8,
    /// `DIM(*AUTO)` 1, `DIM(*VAR)` 2, else 0.
    pub vardim: u8,
    /// Null-capable (`NULLIND`, a file's null-capable field): its null
    /// indicator is the place `IR__null`.
    pub nullind: bool,
    /// `ALTSEQ(*NONE)`: compared by the normal sequence.
    pub altseq_none: bool,
    /// `FROMFILE`: a pre-run-time array's input file.
    pub fromfile: String,
    /// `TOFILE`: the file the array is written to at the end.
    pub tofile: String,
}

/// A data structure's layout.
#[derive(Clone, Debug, Default)]
pub struct Ds {
    /// The structure's symbol.
    pub sym: SymId,
    /// Its length in bytes.
    pub len: u32,
    /// Its subfields, in declaration order.
    pub subfields: Vec<SymId>,
    /// `QUALIFIED` (or a `LIKEDS` copy, which is always qualified).
    pub qualified: bool,
    /// The IR named type.
    pub ir_type: String,
    /// `INZ` on the structure.
    pub inz: bool,
    /// Holds a pointer, so aligned to 16.
    pub has_ptr: bool,
    /// `OCCURS(n)`: a multiple-occurrence structure.
    pub occurs: u32,
    /// The program status data structure.
    pub psds: bool,
    /// A data-area structure (`U` in column 23 or `DTAARA`).
    pub dtaara: Option<String>,
    /// An array without `QUALIFIED`: its subfields are its own, yet
    /// it qualifies no name.
    pub unqualified_array: bool,
}

/// A parameter of a prototype or interface.
#[derive(Clone, Debug, Default)]
pub struct Parm {
    /// Upper-case name (may be empty in a prototype).
    pub name: String,
    /// Its type.
    pub ty: Ty,
    /// Elements, 0 for a scalar.
    pub dim: u32,
    /// `VALUE`.
    pub value: bool,
    /// `CONST`.
    pub konst: bool,
    /// `OPTIONS(*NOPASS)`.
    pub nopass: bool,
    /// `OPTIONS(*OMIT)`.
    pub omit: bool,
    /// `OPTIONS(*VARSIZE)`.
    pub varsize: bool,
    /// `OPTIONS(*STRING)`.
    pub string: bool,
    /// `OPTIONS(*TRIM)`.
    pub trim: bool,
}

/// A prototype or a procedure interface.
#[derive(Clone, Debug, Default)]
pub struct Proto {
    /// Upper-case name.
    pub name: String,
    /// `EXTPGM`: a program called dynamically, by this name.
    pub extpgm: Option<String>,
    /// `EXTPROC`: the external procedure name.
    pub extproc: Option<String>,
    /// `EXTPROC` names a variable (a procedure pointer), not a string.
    pub extproc_word: bool,
    /// The return type, when it returns a value.
    pub ret: Option<Ty>,
    /// Elements of an array return value.
    pub ret_dim: u32,
    /// The parameters.
    pub parms: Vec<Parm>,
    /// `OPDESC`.
    pub opdesc: bool,
    /// `RTNPARM`: the return value travels as a hidden first
    /// parameter, which `%PARMS` counts.
    pub rtnparm: bool,
    /// Where it is declared.
    pub def_line: u32,
}

/// One reference to a symbol, for the cross-reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ref {
    /// Listing line.
    pub line: u32,
    /// The reference changes the symbol.
    pub modified: bool,
}

/// The symbols of a module.
#[derive(Clone, Debug, Default)]
pub struct Table {
    /// Every symbol.
    pub syms: Vec<Sym>,
    /// Data structures.
    pub ds: Vec<Ds>,
    /// Prototypes and interfaces.
    pub protos: Vec<Proto>,
    /// Names by scope.
    scopes: Vec<HashMap<String, SymId>>,
    /// References by symbol.
    pub refs: HashMap<SymId, Vec<Ref>>,
    /// Record formats of the declared files.
    pub recfmts: Vec<crate::extfiles::RecFmt>,
    /// External descriptions `EXTNAME` names, by `LIB/FILE`.
    pub extnames: HashMap<String, crate::files::ExtFile>,
    /// The program's parameters (`*ENTRY PLIST` or a global `DCL-PI`)
    /// by name, as declared.
    pub entry_names: Vec<String>,
    /// The program's parameter fields, in order.
    pub entry: Vec<SymId>,
    /// The pointers they are based on, set by the entry procedure.
    pub entry_ptrs: Vec<SymId>,
    /// Fields and structures tied to a data area (`DTAARA`): the area,
    /// `NAME` or `LIB/NAME`.
    pub dtaaras: HashMap<SymId, String>,
    /// The ones read when the program starts and written when it ends
    /// (`DTAARA(*AUTO)`, a `U` structure).
    pub dtaara_auto: std::collections::BTreeSet<SymId>,
    /// The structures files name as their `INFDS`.
    pub infds: Vec<String>,
    /// `EXTBININT(*YES)`: external binary fields without decimals are
    /// integers.
    pub extbinint: bool,
    /// `ALWNULL(*USRCTL)`: null-capable fields have null indicators.
    pub alwnull_usrctl: bool,
    /// Files dropped because every format was ignored: their names are
    /// undefined without a message of their own.
    pub dropped_files: Vec<String>,
}

impl Table {
    /// An empty table with the global scope.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            ..Self::default()
        }
    }

    /// Open a new scope (a procedure's); its number.
    pub fn new_scope(&mut self) -> usize {
        self.scopes.push(HashMap::new());
        self.scopes.len() - 1
    }

    /// Add a symbol under its name in its scope; `Err` with the
    /// existing symbol when the name is taken there.
    pub fn add(&mut self, s: Sym) -> Result<SymId, SymId> {
        let scope = s.scope;
        let name = s.name.clone();
        if let Some(&old) = self.scopes[scope].get(&name) {
            return Err(old);
        }
        let id = self.syms.len();
        self.syms.push(s);
        if !name.is_empty() {
            self.scopes[scope].insert(name, id);
        }
        Ok(id)
    }

    /// Point a scope's name at another symbol: an `EXTNAME` subfield
    /// takes over the program field of the same name a file declared.
    pub fn rebind(&mut self, scope: usize, name: &str, id: SymId) {
        if let Some(m) = self.scopes.get_mut(scope) {
            m.insert(name.to_string(), id);
        }
    }

    /// Add a symbol that no name finds (a qualified subfield).
    pub fn add_hidden(&mut self, s: Sym) -> SymId {
        self.syms.push(s);
        self.syms.len() - 1
    }

    /// Find a name: in `scope` first, then globally.
    #[must_use]
    pub fn find(&self, scope: usize, name: &str) -> Option<SymId> {
        self.scopes
            .get(scope)
            .and_then(|m| m.get(name))
            .or_else(|| self.scopes[0].get(name))
            .copied()
    }

    /// Find a name in exactly one scope.
    #[must_use]
    pub fn find_in(&self, scope: usize, name: &str) -> Option<SymId> {
        self.scopes.get(scope).and_then(|m| m.get(name)).copied()
    }

    /// A subfield of a structure by name.
    #[must_use]
    pub fn subfield(&self, ds: usize, name: &str) -> Option<SymId> {
        self.ds
            .get(ds)?
            .subfields
            .iter()
            .copied()
            .find(|&s| self.syms[s].name == name)
    }

    /// Note a reference.
    pub fn refer(&mut self, id: SymId, line: u32, modified: bool) {
        self.refs.entry(id).or_default().push(Ref { line, modified });
    }

    /// A structure's length.
    #[must_use]
    pub fn ds_len(&self, i: usize) -> u32 {
        self.ds.get(i).map_or(0, |d| d.len)
    }

    /// A type's size.
    #[must_use]
    pub fn size(&self, t: &Ty) -> u32 {
        t.size(&|i| self.ds_len(i))
    }

    /// A type's IR text.
    #[must_use]
    pub fn ir_type(&self, t: &Ty) -> String {
        t.ir(&|i| self.ds.get(i).map_or_else(|| "char(1)".into(), |d| d.ir_type.clone()))
    }

    /// A symbol's IR storage type: its type, as an array when it has
    /// elements.
    #[must_use]
    pub fn ir_storage(&self, s: &Sym) -> String {
        let t = self.ir_type(&s.ty);
        if s.dim > 0 { format!("array[{}] of {t}", s.dim) } else { t }
    }
}
