// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Strong typing of distinct types, as the primary applies it
//! (`tools/conformance/sql/cases/objects2/TYPES`, `TYPES2`, `TYPES3`).
//! A value of a distinct type is its source type's value; its type is
//! known where expressions are typed (`TypeSpec::udt`), and:
//!
//! - it compares only with its own type (`SQL0401`), and `LIKE` takes
//!   none (`SQL0131`);
//! - no arithmetic operator takes it (`SQL0402`), no concatenation and no
//!   built-in function (`SQL0171`; `SQL0402` for the numeric casting
//!   functions), and no aggregate but `COUNT`;
//! - the generated functions convert: `TYPE(source)` makes one from a
//!   value that promotes to the source (else `SQL0204`), and the source
//!   type's name (`DECIMAL`, `CHARACTER`, ...) gives the source back, in
//!   the type's schema or through the path;
//! - `CAST` goes to its source type with any length (else `SQL0461`) and
//!   comes from a value of a type of the source's kind;
//! - a `CASE` result, `COALESCE` or `UNION` mixes it with nothing else
//!   (`SQL0581`, `SQL0171`, `SQL0415`); an assignment takes it into its
//!   own type or the source's (`SQL0408`).

use std::borrow::Cow;

use super::registry::{self, Distinct};
use crate::ast::{DataType, TypeSpec};
use crate::error::{Result, SqlError};
use crate::exec::restype::SqlType;

fn coded(code: i32, state: &'static str, what: String) -> SqlError {
    SqlError::Coded { code, state, what }
}

/// The name IBM gives a source type, and its generated function from the
/// distinct type (`SYSTYPES.SOURCE_TYPE`).
#[must_use]
pub fn source_function(t: &DataType) -> &'static str {
    match t {
        DataType::Char(_) => "CHARACTER",
        DataType::Varchar(_) => "VARCHAR",
        DataType::Graphic(_) => "GRAPHIC",
        DataType::Vargraphic(_) => "VARGRAPHIC",
        DataType::Decimal(..) => "DECIMAL",
        DataType::Numeric(..) => "NUMERIC",
        DataType::SmallInt => "SMALLINT",
        DataType::Integer => "INTEGER",
        DataType::BigInt => "BIGINT",
        DataType::Real => "REAL",
        DataType::Double => "DOUBLE",
        DataType::DecFloat(_) => "DECFLOAT",
        DataType::Date => "DATE",
        DataType::Time => "TIME",
        DataType::Timestamp(_) => "TIMESTAMP",
        DataType::Binary(_) => "BINARY",
        DataType::Varbinary(_) => "VARBINARY",
        DataType::Boolean => "BOOLEAN",
        DataType::Blob(_) => "BLOB",
        DataType::Clob(_) => "CLOB",
        DataType::Dbclob(_) => "DBCLOB",
    }
}

/// `SQL0204`: no such type.
pub fn not_found(name: &str) -> SqlError {
    SqlError::ObjectNotFound {
        name: name.to_string(),
        kind: "type",
    }
}

/// A type as written resolved: a distinct type's name to its source type,
/// marked as the type.
pub fn resolve(spec: &TypeSpec) -> Result<Cow<'_, TypeSpec>> {
    let Some(name) = &spec.udt else {
        return Ok(Cow::Borrowed(spec));
    };
    let d = registry::find(name).ok_or_else(|| not_found(name))?;
    Ok(Cow::Owned(d.spec()))
}

fn udt(t: Option<&SqlType>) -> Option<&str> {
    t.and_then(|t| t.spec.udt.as_deref())
}

fn same_kind(a: &DataType, b: &DataType) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

fn text(t: &DataType) -> bool {
    matches!(
        t,
        DataType::Char(_) | DataType::Varchar(_) | DataType::Clob(_)
    )
}

fn graphic(t: &DataType) -> bool {
    matches!(
        t,
        DataType::Graphic(_) | DataType::Vargraphic(_) | DataType::Dbclob(_)
    )
}

fn binary(t: &DataType) -> bool {
    matches!(
        t,
        DataType::Binary(_) | DataType::Varbinary(_) | DataType::Blob(_)
    )
}

fn number_rank(t: &DataType) -> Option<u8> {
    Some(match t {
        DataType::SmallInt => 0,
        DataType::Integer => 1,
        DataType::BigInt => 2,
        DataType::Decimal(..) | DataType::Numeric(..) => 3,
        DataType::Real => 4,
        DataType::Double => 5,
        DataType::DecFloat(_) => 6,
        _ => return None,
    })
}

/// A value of `from` may be cast to a distinct type of source `to`.
fn castable(from: &DataType, to: &DataType) -> bool {
    (number_rank(from).is_some() && number_rank(to).is_some())
        || (text(from) && text(to))
        || (graphic(from) && graphic(to))
        || (binary(from) && binary(to))
        || same_kind(from, to)
}

/// An argument of `from` promotes to a parameter of `to` (function
/// resolution).
fn promotes(from: &DataType, to: &DataType) -> bool {
    if let (Some(a), Some(b)) = (number_rank(from), number_rank(to)) {
        return a <= b;
    }
    let short = |t: &DataType| matches!(t, DataType::Char(_) | DataType::Varchar(_));
    (short(from) && (short(to) || matches!(to, DataType::Clob(_))))
        || (graphic(from) && graphic(to))
        || (binary(from) && binary(to))
        || same_kind(from, to)
}

/// `CAST(x AS to)` with `x` of type `from`: the type cast to, or the
/// casts IBM refuses with a distinct type (`SQL0461`).
pub fn cast(from: Option<&SqlType>, to: &TypeSpec) -> Result<TypeSpec> {
    let to = resolve(to)?.into_owned();
    let Some(f) = from else { return Ok(to) };
    let refused = || {
        coded(
            -461,
            "42846",
            format!(
                "a value of {} cannot be cast to {}",
                describe(f),
                to.udt
                    .clone()
                    .unwrap_or_else(|| crate::exec::cast::type_name(&to.data_type).into())
            ),
        )
    };
    match (f.spec.udt.as_deref(), to.udt.as_deref()) {
        (Some(a), Some(b)) if a != b => Err(refused()),
        (Some(_), None) if !same_kind(f.data_type(), &to.data_type) => Err(refused()),
        (None, Some(_)) if !castable(f.data_type(), &to.data_type) => Err(refused()),
        _ => Ok(to),
    }
}

fn describe(t: &SqlType) -> String {
    t.spec
        .udt
        .clone()
        .unwrap_or_else(|| crate::exec::cast::type_name(t.data_type()).into())
}

/// A comparison of `a` with `b` (`=`, `IN`, `BETWEEN`, a quantified
/// subquery, `IS DISTINCT FROM`): only a distinct type with itself.
pub fn compare(a: Option<&SqlType>, b: Option<&SqlType>) -> Result<()> {
    let (x, y) = (udt(a), udt(b));
    if (x.is_some() || y.is_some()) && x != y && a.is_some() && b.is_some() {
        return Err(coded(
            -401,
            "42818",
            format!(
                "the operands of a comparison are {} and {}",
                a.map(describe).unwrap_or_default(),
                b.map(describe).unwrap_or_default()
            ),
        ));
    }
    Ok(())
}

/// `LIKE` over `a`.
pub fn like(a: Option<&SqlType>) -> Result<()> {
    match udt(a) {
        Some(u) => Err(coded(
            -131,
            "42818",
            format!("LIKE does not take an operand of distinct type {u}"),
        )),
        None => Ok(()),
    }
}

/// An arithmetic operator (`SQL0402`) or concatenation (`SQL0171`) over
/// `a` and `b`.
pub fn operator(a: Option<&SqlType>, b: Option<&SqlType>, concat: bool) -> Result<()> {
    let Some(u) = udt(a).or(udt(b)) else {
        return Ok(());
    };
    if concat {
        return Err(coded(
            -171,
            "42815",
            format!("CONCAT does not take an argument of distinct type {u}"),
        ));
    }
    Err(coded(
        -402,
        "42819",
        format!("an arithmetic operand is of distinct type {u}"),
    ))
}

/// The numeric casting functions, which IBM refuses as arithmetic.
const NUMERIC_CASTS: &[&str] = &[
    "DECIMAL", "DEC", "NUMERIC", "ZONED", "INTEGER", "INT", "SMALLINT", "BIGINT", "DOUBLE",
    "FLOAT", "REAL", "DECFLOAT",
];

/// What a call of `name` with arguments of `args` is when a distinct type
/// takes part: the result type of a generated function, an error for a
/// function that takes no distinct type, or `None` to type it as usual.
pub fn function(
    name: &str,
    args: &[Option<SqlType>],
    distinct_args: bool,
) -> Result<Option<SqlType>> {
    if let Some(d) = to_type(name, args.len()) {
        let a = args.first().and_then(Option::as_ref);
        return match a {
            Some(t)
                if t.spec.udt.as_deref().is_some_and(|u| u != d.qualified())
                    || (t.spec.udt.is_none() && !promotes(t.data_type(), &d.source.data_type)) =>
            {
                Err(not_found(name))
            }
            // A function's result may be null (an overflow is SQL0802's null).
            _ => Ok(Some(SqlType {
                spec: d.spec(),
                nullable: true,
            })),
        };
    }
    let Some(u) = args.iter().find_map(|a| udt(a.as_ref())) else {
        return Ok(None);
    };
    // `IS [NOT] DISTINCT FROM` is a comparison.
    if let ("*DISTINCT" | "*NOTDISTINCT", [a, b]) = (name, args) {
        compare(a.as_ref(), b.as_ref())?;
        return Ok(None);
    }
    let d = registry::by_qualified(u);
    if let (Some(d), [Some(t)]) = (&d, args)
        && is_to_source(name, d)
    {
        return Ok(Some(SqlType {
            spec: d.source.clone(),
            nullable: t.nullable,
        }));
    }
    let base = name.rsplit_once('.').map_or(name, |(_, n)| n);
    // A function of a user's schema that takes no such argument.
    if name
        .split_once('.')
        .is_some_and(|(s, _)| !matches!(s, "SYSIBM" | "QSYS" | "QSYS2" | "SYSPROC" | "SYSFUN"))
    {
        return Err(not_found(name));
    }
    if base == "COUNT" && !distinct_args {
        return Ok(None);
    }
    if NUMERIC_CASTS.contains(&base) {
        return Err(coded(
            -402,
            "42819",
            format!("{base} does not take an argument of distinct type {u}"),
        ));
    }
    Err(coded(
        -171,
        "42815",
        format!("{base} does not take an argument of distinct type {u}"),
    ))
}

/// The distinct type a function name of `n` arguments makes: the type's
/// own name.
fn to_type(name: &str, n: usize) -> Option<Distinct> {
    if n != 1 {
        return None;
    }
    registry::find(name)
}

/// Whether `name` is `d`'s generated function back to its source.
fn is_to_source(name: &str, d: &Distinct) -> bool {
    let want = source_function(&d.source.data_type);
    match name.split_once('.') {
        Some((s, n)) => n == want && d.schema.to_string().trim_end() == s,
        None => name == want && registry::in_path(d.lib),
    }
}

/// What evaluating a one-argument call of `name` is, the argument of
/// type `arg`: a cast to a distinct type's source, the argument itself
/// (back to the source), or the usual function.
pub enum Call {
    /// Cast the argument to this type.
    To(TypeSpec),
    /// The argument as it is.
    Same,
}

/// How a call of `name` on one argument of type `arg` runs, when a
/// distinct type takes part.
pub fn call(name: &str, arg: impl FnOnce() -> Option<SqlType>) -> Option<Call> {
    if !registry::any() {
        return None;
    }
    if let Some(d) = to_type(name, 1) {
        return Some(Call::To(d.source));
    }
    let t = arg()?;
    let d = registry::by_qualified(t.spec.udt.as_deref()?)?;
    is_to_source(name, &d).then_some(Call::Same)
}

/// An assignment of a value of `value` to a column of `column`'s type:
/// into its own distinct type, or a distinct type's value into its source
/// type (`SQL0408`).
pub fn assign(column: Option<&str>, value: Option<&SqlType>) -> Result<()> {
    match (column, udt(value)) {
        (Some(c), Some(v)) if c != v => Err(coded(
            -408,
            "42821",
            format!(
                "a value of distinct type {v} cannot be assigned to a column of distinct type {c}"
            ),
        )),
        _ => Ok(()),
    }
}

/// `CASE`, `COALESCE` or `UNION` results of `a` and `b` with different
/// distinct types: the error, by where.
pub fn mixed(a: &SqlType, b: &SqlType, what: Mixed) -> Option<SqlError> {
    if a.spec.udt == b.spec.udt {
        return None;
    }
    let m = format!("{} and {} have no common type", describe(a), describe(b));
    Some(match what {
        Mixed::Case => coded(-581, "42804", m),
        Mixed::Coalesce => coded(-171, "42815", m),
        Mixed::Union => coded(-415, "42825", m),
    })
}

/// Where results are combined.
#[derive(Clone, Copy)]
pub enum Mixed {
    /// A `CASE` expression's results.
    Case,
    /// `COALESCE`, `VALUE`, `IFNULL` arguments or `VALUES` rows.
    Coalesce,
    /// `UNION` columns.
    Union,
}

/// The comparisons, `LIKE`s and operators of a condition (`WHERE`,
/// `HAVING`) checked against these rules before its rows are kept.
/// BLOCKED(rel): a quantified or `IN` subquery's column is not typed
/// here, so a distinct type compared with another type's column of a
/// subquery is not refused.
#[allow(clippy::many_single_char_names)]
pub fn check(e: &crate::ast::Expr, scope: &crate::exec::restype::Scope<'_>) -> Result<()> {
    use crate::ast::Expr;
    // A name the joined tables share (a USING column) is the scan's to
    // settle; its type is not needed here.
    let type_in =
        |e: &Expr, scope: &crate::exec::restype::Scope<'_>| match crate::exec::restype::type_in(
            e, scope,
        ) {
            Err(SqlError::AmbiguousColumn { .. }) => Ok(None),
            r => r,
        };
    match e {
        Expr::And(a, b) | Expr::Or(a, b) => {
            check(a, scope)?;
            check(b, scope)
        }
        Expr::Not(a) => check(a, scope),
        Expr::Compare(a, _, b, None) => {
            let (x, y) = (type_in(a, scope)?, type_in(b, scope)?);
            marker(a, y.as_ref())?;
            marker(b, x.as_ref())?;
            compare(x.as_ref(), y.as_ref())
        }
        Expr::Between(a, lo, hi, _) => {
            let t = type_in(a, scope)?;
            compare(t.as_ref(), type_in(lo, scope)?.as_ref())?;
            compare(t.as_ref(), type_in(hi, scope)?.as_ref())
        }
        Expr::InList(a, list, _) => {
            let t = type_in(a, scope)?;
            for x in list {
                compare(t.as_ref(), type_in(x, scope)?.as_ref())?;
            }
            Ok(())
        }
        Expr::Like(a, p, _, _) => {
            like(type_in(a, scope)?.as_ref())?;
            type_in(p, scope).map(|_| ())
        }
        Expr::IsNull(a, _) => type_in(a, scope).map(|_| ()),
        Expr::Compare(..) | Expr::InSubquery(..) | Expr::Exists(..) | Expr::Subquery(_) => Ok(()),
        e => type_in(e, scope).map(|_| ()),
    }
}

/// Whether `name` is a function generated for a distinct type: the
/// type's own name, or its source type's in the type's schema.
#[must_use]
pub fn is_generated(name: &crate::ast::ObjectName) -> bool {
    let full = match &name.schema {
        Some(s) => format!("{}.{}", s.value, name.name.value),
        None => name.name.value.clone(),
    };
    registry::find(&full).is_some()
        || name
            .schema
            .as_ref()
            .is_some_and(|s| registry::with_source_in(&s.value, &name.name.value))
}

/// `SQL0432`: an untyped parameter marker beside a distinct type.
fn marker(e: &crate::ast::Expr, other: Option<&SqlType>) -> Result<()> {
    match (e, udt(other)) {
        (crate::ast::Expr::Param(_), Some(u)) => Err(coded(
            -432,
            "42841",
            format!("a parameter marker cannot have distinct type {u}; CAST it"),
        )),
        _ => Ok(()),
    }
}
