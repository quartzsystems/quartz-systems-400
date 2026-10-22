// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the compiler knows before it generates code: the variables
//! with their types and storage, the labels, the subroutines, the
//! files declared, and the type of every expression.

use std::collections::HashMap;

use crate::ast::{Expr, Kind, Lit, Op, Program, Stg, VarType};
use crate::diag::{Diag, ids};

/// A value's type as the generator sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    /// Character, with its length.
    Char(u32),
    /// Decimal with digits and positions (integers are decimals with
    /// no positions).
    Dec(u8, u8),
    /// Logical.
    Lgl,
    /// Pointer.
    Ptr,
}

impl Ty {
    /// Whether the type is numeric.
    #[must_use]
    pub fn is_num(&self) -> bool {
        matches!(self, Self::Dec(..))
    }
}

/// A declared variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Var {
    /// Its name, upper case.
    pub name: String,
    /// Its CL type.
    pub ty: VarType,
    /// Length in characters, or digits.
    pub len: u32,
    /// Decimal positions.
    pub dec: u8,
    /// Its storage.
    pub stg: Stg,
    /// Its initial value.
    pub value: Option<Lit>,
    /// A parameter of the program.
    pub parm: bool,
    /// Declared on which line.
    pub line: usize,
    /// A field of a declared file: the file's number.
    pub file: Option<usize>,
}

impl Var {
    /// The QIR type of the variable's storage.
    #[must_use]
    pub fn qir_type(&self) -> String {
        match self.ty {
            VarType::Char => format!("char({})", self.len),
            VarType::Dec => format!("packed({},{})", self.len, self.dec),
            VarType::Lgl => "ind".into(),
            VarType::Int => if self.len <= 2 { "i16" } else { "i32" }.into(),
            VarType::Uint => if self.len <= 2 { "u16" } else { "u32" }.into(),
            VarType::Ptr => "ptr".into(),
        }
    }

    /// The expression type of the variable.
    #[must_use]
    pub fn ty(&self) -> Ty {
        match self.ty {
            VarType::Char => Ty::Char(self.len),
            VarType::Dec => Ty::Dec(u8::try_from(self.len).unwrap_or(15), self.dec),
            VarType::Lgl => Ty::Lgl,
            VarType::Int | VarType::Uint => Ty::Dec(if self.len <= 2 { 5 } else { 10 }, 0),
            VarType::Ptr => Ty::Ptr,
        }
    }

    /// The place in the IR.
    #[must_use]
    pub fn place(&self) -> String {
        format!("${}", self.name)
    }
}

/// A record format of a declared file, as the generator lays it out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Format {
    /// Name.
    pub name: String,
    /// Record length.
    pub len: u32,
    /// Fields: name, DDS type, position (1-based), bytes, digits,
    /// decimals.
    pub fields: Vec<FieldDef>,
}

/// A field of a format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldDef {
    /// Name.
    pub name: String,
    /// DDS data type: `A`, `P`, `S`, `B`, `L`, `T`, `Z`...
    pub ty: char,
    /// Buffer position, 1-based.
    pub pos: u32,
    /// Bytes.
    pub len: u32,
    /// Digits.
    pub digits: u32,
    /// Decimal positions.
    pub decimals: u32,
}

/// A declared file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    /// `DCLF`'s number (0-based).
    pub n: usize,
    /// The file.
    pub name: String,
    /// Its library.
    pub lib: String,
    /// `OPNID`, the file name when none.
    pub opnid: String,
    /// A display file.
    pub display: bool,
    /// The formats declared.
    pub formats: Vec<Format>,
}

/// Where declared files come from at compile time.
pub trait Files {
    /// The file's formats and whether it is a display file; `None`
    /// when there is no such file.
    fn file(&mut self, name: &str, lib: &str) -> Option<(bool, Vec<Format>)>;
}

/// No files at all.
pub struct NoFiles;

impl Files for NoFiles {
    fn file(&mut self, _name: &str, _lib: &str) -> Option<(bool, Vec<Format>)> {
        None
    }
}

/// The tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tables {
    /// Variables by name.
    pub vars: HashMap<String, Var>,
    /// Declaration order.
    pub order: Vec<String>,
    /// Labels to statement numbers.
    pub labels: HashMap<String, usize>,
    /// Subroutines to statement numbers.
    pub subrs: HashMap<String, usize>,
    /// Files.
    pub files: Vec<File>,
    /// The program's parameters in order.
    pub parms: Vec<String>,
    /// Where the first executable statement is.
    pub first_exec: Option<usize>,
}

impl Tables {
    /// A variable.
    #[must_use]
    pub fn var(&self, name: &str) -> Option<&Var> {
        self.vars.get(name)
    }
}

/// Build the tables; diagnostics for what is wrong.
pub fn tables(p: &Program, files: &mut dyn Files, errs: &mut Vec<Diag>) -> Tables {
    let mut t = Tables::default();
    let mut declared_done = false;
    for s in &p.stmts {
        if let Some(l) = &s.label
            && t.labels.insert(l.clone(), s.n).is_some()
        {
            errs.push(Diag::at(
                s.line,
                ids::NO_LABEL,
                format!("label {l} is defined twice"),
            ));
        }
        match &s.kind {
            Kind::Pgm(parms) => {
                if s.n != 1 {
                    errs.push(Diag::at(
                        s.line,
                        ids::PGM_FIRST,
                        "PGM must be the first statement",
                    ));
                }
                t.parms.clone_from(parms);
            }
            Kind::Dcl(d) => {
                if declared_done {
                    errs.push(Diag::at(
                        s.line,
                        ids::DECL_ORDER,
                        "declarations come before the first command",
                    ));
                }
                let (len, dec) = match d.ty {
                    VarType::Char => (d.len.unwrap_or(32), 0),
                    VarType::Dec => (d.len.unwrap_or(15), d.dec.unwrap_or(5)),
                    VarType::Lgl => (1, 0),
                    VarType::Int | VarType::Uint => (d.len.unwrap_or(4), 0),
                    VarType::Ptr => (16, 0),
                };
                if d.ty == VarType::Char && !(1..=5000).contains(&len) {
                    errs.push(Diag::at(
                        s.line,
                        ids::BAD_VALUE,
                        format!("&{}: a *CHAR variable is 1 to 5000 long", d.var),
                    ));
                }
                if d.ty == VarType::Dec && (len > 15 || u32::from(dec) > len || dec > 9) {
                    errs.push(Diag::at(
                        s.line,
                        ids::BAD_VALUE,
                        format!(
                            "&{}: a *DEC variable has up to 15 digits and 9 positions",
                            d.var
                        ),
                    ));
                }
                let v = Var {
                    name: d.var.clone(),
                    ty: d.ty,
                    len,
                    dec,
                    stg: d.stg.clone(),
                    value: d.value.clone(),
                    parm: t.parms.contains(&d.var),
                    line: s.line,
                    file: None,
                };
                if t.vars.insert(d.var.clone(), v).is_some() {
                    errs.push(Diag::at(
                        s.line,
                        ids::REDECLARED,
                        format!("&{} is declared twice", d.var),
                    ));
                } else {
                    t.order.push(d.var.clone());
                }
            }
            Kind::DclF(f) => {
                if declared_done {
                    errs.push(Diag::at(
                        s.line,
                        ids::DECL_ORDER,
                        "declarations come before the first command",
                    ));
                }
                let n = t.files.len();
                let Some((display, formats)) = files.file(&f.file, &f.lib) else {
                    errs.push(Diag::at(
                        s.line,
                        ids::BAD_VALUE,
                        format!("file {}/{} was not found", f.lib, f.file),
                    ));
                    continue;
                };
                let formats: Vec<Format> = if f.rcdfmt.is_empty() {
                    formats
                } else {
                    formats
                        .into_iter()
                        .filter(|x| f.rcdfmt.contains(&x.name))
                        .collect()
                };
                for fmt in &formats {
                    for fd in &fmt.fields {
                        let (ty, len, dec) = match fd.ty {
                            'P' | 'S' | 'B' => (
                                VarType::Dec,
                                fd.digits.min(15),
                                u8::try_from(fd.decimals).unwrap_or(0),
                            ),
                            _ => (VarType::Char, fd.len, 0),
                        };
                        let v = Var {
                            name: fd.name.clone(),
                            ty,
                            len,
                            dec,
                            stg: Stg::Auto,
                            value: None,
                            parm: false,
                            line: s.line,
                            file: Some(n),
                        };
                        if !t.vars.contains_key(&fd.name) {
                            t.order.push(fd.name.clone());
                            t.vars.insert(fd.name.clone(), v);
                        }
                    }
                }
                t.files.push(File {
                    n,
                    name: f.file.clone(),
                    lib: f.lib.clone(),
                    opnid: f.opnid.clone().unwrap_or_else(|| f.file.clone()),
                    display,
                    formats,
                });
            }
            Kind::DclPrcOpt(_) | Kind::EndPgm => {}
            Kind::MonMsg(..) if !declared_done => {}
            Kind::Subr(name) => {
                declared_done = true;
                if t.subrs.insert(name.clone(), s.n).is_some() {
                    errs.push(Diag::at(
                        s.line,
                        ids::SUBR,
                        format!("subroutine {name} is defined twice"),
                    ));
                }
                t.first_exec.get_or_insert(s.n);
            }
            _ => {
                declared_done = true;
                t.first_exec.get_or_insert(s.n);
            }
        }
    }
    for p in &t.parms {
        match t.vars.get_mut(p) {
            Some(v) => v.parm = true,
            None => errs.push(Diag::at(
                1,
                ids::UNDECLARED,
                format!("parameter &{p} is not declared"),
            )),
        }
    }
    t
}

/// The type of an expression, or a diagnostic.
pub fn type_of(t: &Tables, e: &Expr) -> Result<Ty, String> {
    Ok(match e {
        Expr::Lit(Lit::Str(s)) => Ty::Char(u32::try_from(s.chars().count().max(1)).unwrap_or(1)),
        Expr::Lit(Lit::Hex(b)) => Ty::Char(u32::try_from(b.len().max(1)).unwrap_or(1)),
        Expr::Lit(Lit::Num(n)) => {
            let d = qdecimal::Decimal::parse(n).map_err(|_| format!("{n} is not a number"))?;
            Ty::Dec(d.digits_needed().max(1), d.scale())
        }
        Expr::Word(w) => Ty::Char(u32::try_from(w.chars().count().max(1)).unwrap_or(1)),
        Expr::Var(v) => t
            .var(v)
            .ok_or_else(|| format!("variable &{v} is not declared"))?
            .ty(),
        Expr::Neg(a) => {
            let ty = type_of(t, a)?;
            if !ty.is_num() {
                return Err("a sign needs a number".into());
            }
            ty
        }
        Expr::Not(a) => {
            if type_of(t, a)? != Ty::Lgl {
                return Err("*NOT needs a logical value".into());
            }
            Ty::Lgl
        }
        Expr::Bin(op, a, b) => {
            let (ta, tb) = (type_of(t, a)?, type_of(t, b)?);
            match op {
                Op::Add | Op::Sub | Op::Mul | Op::Div => match (&ta, &tb) {
                    (Ty::Dec(da, sa), Ty::Dec(db, sb)) => arith(*op, *da, *sa, *db, *sb),
                    _ => return Err("arithmetic needs numbers".into()),
                },
                Op::Cat | Op::Bcat | Op::Tcat => match (&ta, &tb) {
                    (Ty::Char(la), Ty::Char(lb)) => {
                        Ty::Char((la + lb + u32::from(*op == Op::Bcat)).min(5000))
                    }
                    _ => return Err("*CAT needs character values".into()),
                },
                Op::Eq | Op::Ne | Op::Gt | Op::Lt | Op::Ge | Op::Le | Op::Ng | Op::Nl => {
                    let same = matches!(
                        (&ta, &tb),
                        (Ty::Char(_), Ty::Char(_))
                            | (Ty::Dec(..), Ty::Dec(..))
                            | (Ty::Lgl, Ty::Lgl)
                            | (Ty::Ptr, Ty::Ptr)
                    );
                    if !same {
                        return Err("the compared values are not of one type".into());
                    }
                    Ty::Lgl
                }
                Op::And | Op::Or => {
                    if ta != Ty::Lgl || tb != Ty::Lgl {
                        return Err("*AND and *OR need logical values".into());
                    }
                    Ty::Lgl
                }
            }
        }
        Expr::Builtin(name, args) => builtin_type(t, name, args)?,
    })
}

/// The intermediate precision of an arithmetic result: CL keeps up to
/// 15 digits and the larger scale.
fn arith(op: Op, da: u8, sa: u8, db: u8, sb: u8) -> Ty {
    let (ia, ib) = (da.saturating_sub(sa), db.saturating_sub(sb));
    let (digits, scale) = match op {
        Op::Add | Op::Sub => (ia.max(ib) + 1 + sa.max(sb), sa.max(sb)),
        Op::Mul => (ia + ib + sa + sb, sa + sb),
        _ => (ia + sb + 9, 9),
    };
    let scale = scale.min(9);
    Ty::Dec(digits.clamp(1, 15).max(scale), scale)
}

fn builtin_type(t: &Tables, name: &str, args: &[Expr]) -> Result<Ty, String> {
    Ok(match name {
        "SST" | "SUBSTRING" => {
            let base = type_of(t, &args[0])?;
            let Ty::Char(n) = base else {
                return Err("%SST needs a character variable".into());
            };
            match &args[2] {
                Expr::Lit(Lit::Num(l)) => Ty::Char(l.parse().unwrap_or(n).min(n)),
                _ => Ty::Char(n),
            }
        }
        "BIN" | "BINARY" => Ty::Dec(10, 0),
        "CHAR" => match type_of(t, &args[0])? {
            Ty::Dec(d, s) => Ty::Char(u32::from(d) + 2 + u32::from(s > 0)),
            Ty::Char(n) => Ty::Char(n),
            Ty::Lgl => Ty::Char(1),
            Ty::Ptr => return Err("%CHAR does not take a pointer".into()),
        },
        "DEC" => match args.len() {
            3 => {
                let d = match &args[1] {
                    Expr::Lit(Lit::Num(n)) => n.parse().unwrap_or(15),
                    _ => 15,
                };
                let s = match &args[2] {
                    Expr::Lit(Lit::Num(n)) => n.parse().unwrap_or(5),
                    _ => 5,
                };
                Ty::Dec(d, s)
            }
            _ => Ty::Dec(15, 5),
        },
        "INT" | "UINT" => Ty::Dec(10, 0),
        "LEN" | "SCAN" | "CHECK" | "CHECKR" | "SIZE" | "OFFSET" | "PARMS" => Ty::Dec(10, 0),
        "SWITCH" => Ty::Lgl,
        "TRIM" | "TRIML" | "TRIMR" | "UPPER" | "LOWER" => type_of(t, &args[0])?,
        "ADDRESS" => Ty::Ptr,
        other => return Err(format!("%{other} is not a built-in function")),
    })
}
