// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The services' shapes as pub400 describes them: each view's columns,
//! each routine's parameters and result columns. `shapes.txt` is made
//! from the capture `tools/conformance/sql/expected/services/SVCCOLS.out`
//! (`tests/service_shapes.rs` checks it and rewrites it).

use std::collections::HashMap;
use std::sync::OnceLock;

use dds::{DataType, FieldDef};
use qsname::Name;

use super::super::super::eval::ColInfo;

/// The shapes, one record per line (see [`parse`]).
pub const TEXT: &str = include_str!("shapes.txt");

/// The plan cache procedures, from pub400's `SYSPARMS`
/// (`tools/conformance/sql/oracle/probes/perf/p2.out`); kept apart from
/// the capture `shapes.txt` is generated from. The defaults of the
/// optional parameters are null, as IBM documents them.
pub const PLAN: &str = include_str!("shapes_plan.txt");

/// A column, parameter or result type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Col {
    /// The SQL name; empty for a scalar function's result.
    pub name: String,
    /// The system name, when the oracle gave one.
    pub system: String,
    /// `VARCHAR`, `INTEGER`, `TIMESTMP` ... as `SYSCOLUMNS` spells them.
    pub ty: String,
    /// Characters, digits, or a timestamp's precision.
    pub len: u32,
    /// Decimal places.
    pub scale: u8,
    /// Nullable.
    pub nullable: bool,
    /// CCSID, 0 for none.
    pub ccsid: u16,
    /// A parameter's mode (`IN`, `OUT`, `INOUT`) and default text.
    pub mode: String,
    /// A parameter's default, as SQL text; empty for none.
    pub default: String,
}

/// A view or routine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    /// `QSYS2` or `SYSTOOLS`.
    pub schema: String,
    /// The SQL name.
    pub name: String,
    /// The specific name of a routine; the system name of a view.
    pub specific: String,
    /// `V` view, `T` table function, `S` scalar function, `P` procedure.
    pub kind: char,
    /// The columns: a view's, a table function's result.
    pub cols: Vec<Col>,
    /// A routine's parameters.
    pub parms: Vec<Col>,
    /// A scalar function's result.
    pub result: Option<Col>,
}

impl Col {
    /// The column as the engine describes it, correlation `alias`.
    #[must_use]
    pub fn info(&self, alias: &str, nullable: bool) -> ColInfo {
        let (ty, len, dec, varying) = engine_type(&self.ty, self.len, self.scale);
        let short = if self.system.is_empty() { &self.name } else { &self.system };
        let mut f = FieldDef::new(
            Name::parse(&short[..short.len().min(10)]).unwrap_or_else(|_| Name::parse("COL").expect("valid")),
            ty,
            len,
            dec,
        );
        f.int_range = ty == DataType::Binary;
        if varying {
            f.varlen = Some(dds::VarLen { allocated: None });
        }
        if self.ccsid != 0 && self.ccsid != 65535 {
            f.ccsid = Some(self.ccsid);
        }
        f.allow_null = nullable;
        if self.name.len() > 10 || self.name != short.trim() {
            f.alias = Some(self.name.clone());
        }
        ColInfo { name: self.name.clone(), table: Some(alias.to_string()), field: Some(f), nullable, hidden: false }
    }
}

/// The engine's type for a shape type: data type, length, decimals,
/// varying.
fn engine_type(ty: &str, len: u32, scale: u8) -> (DataType, u32, u8, bool) {
    match ty {
        "CHAR" => (DataType::Char, len, 0, false),
        "VARCHAR" => (DataType::Char, len, 0, true),
        "GRAPHIC" => (DataType::Graphic, len, 0, false),
        "VARG" => (DataType::Graphic, len, 0, true),
        "CLOB" => (DataType::Clob, len, 0, false),
        "DBCLOB" => (DataType::Dbclob, len, 0, false),
        "BLOB" => (DataType::Blob, len, 0, false),
        "BINARY" => (DataType::Bytes, len, 0, false),
        "VARBIN" => (DataType::Bytes, len, 0, true),
        "SMALLINT" => (DataType::Binary, 4, 0, false),
        "INTEGER" => (DataType::Binary, 9, 0, false),
        "BIGINT" => (DataType::Binary, 18, 0, false),
        "NUMERIC" => (DataType::Zoned, len, scale, false),
        "DATE" => (DataType::Date, 10, 0, false),
        "TIME" => (DataType::Time, 8, 0, false),
        "TIMESTMP" => (DataType::Timestamp, if len == 0 { 19 } else { 20 + len }, 0, false),
        "DOUBLE" | "FLOAT" => (DataType::Float, 8, 0, false),
        // XML has no engine type: its text, as XMLSERIALIZE gives it.
        "XML" => (DataType::Clob, 2_147_483_647, 0, false),
        _ => (DataType::Packed, len, scale, false),
    }
}

/// Parse the shapes text:
///
/// ```text
/// V schema name system              a view
/// F schema name specific T|S|P      a routine
/// c name system type len scale Y|N ccsid      a column
/// p mode name type len scale Y|N ccsid default...   a parameter
/// r type len scale ccsid            a scalar function's result
/// ```
#[must_use]
pub fn parse(text: &str) -> Vec<Shape> {
    let mut out: Vec<Shape> = Vec::new();
    for line in text.lines() {
        let w: Vec<&str> = line.split(' ').collect();
        let num = |i: usize| w.get(i).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
        match w.first().copied() {
            Some("V") if w.len() >= 4 => out.push(Shape {
                schema: w[1].into(),
                name: w[2].into(),
                specific: w[3].into(),
                kind: 'V',
                ..Shape::default()
            }),
            Some("F") if w.len() >= 5 => out.push(Shape {
                schema: w[1].into(),
                name: w[2].into(),
                specific: w[3].into(),
                kind: w[4].chars().next().unwrap_or('S'),
                ..Shape::default()
            }),
            Some("c") if w.len() >= 8 => {
                if let Some(s) = out.last_mut() {
                    s.cols.push(Col {
                        name: w[1].into(),
                        system: if w[2] == "-" { String::new() } else { w[2].into() },
                        ty: w[3].into(),
                        len: num(4),
                        scale: u8::try_from(num(5)).unwrap_or(0),
                        nullable: w[6] == "Y",
                        ccsid: u16::try_from(num(7)).unwrap_or(0),
                        mode: String::new(),
                        default: String::new(),
                    });
                }
            }
            Some("p") if w.len() >= 9 => {
                if let Some(s) = out.last_mut() {
                    // An unnamed parameter is known by its position.
                    let name = if w[2].is_empty() { format!("#{}", s.parms.len() + 1) } else { w[2].to_string() };
                    s.parms.push(Col {
                        name,
                        system: String::new(),
                        ty: w[3].into(),
                        len: num(4),
                        scale: u8::try_from(num(5)).unwrap_or(0),
                        nullable: w[6] == "Y",
                        ccsid: u16::try_from(num(7)).unwrap_or(0),
                        mode: w[1].into(),
                        default: w[8..].join(" "),
                    });
                }
            }
            Some("r") if w.len() >= 5 => {
                if let Some(s) = out.last_mut() {
                    s.result = Some(Col {
                        name: String::new(),
                        system: String::new(),
                        ty: w[1].into(),
                        len: num(2),
                        scale: u8::try_from(num(3)).unwrap_or(0),
                        nullable: true,
                        ccsid: u16::try_from(num(4)).unwrap_or(0),
                        mode: String::new(),
                        default: String::new(),
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// The shapes by `schema.name` (views) and `schema.specific`
/// (routines); the routines of one name in their order by name too.
struct Index {
    views: HashMap<String, Shape>,
    routines: HashMap<String, Vec<Shape>>,
}

fn index() -> &'static Index {
    static I: OnceLock<Index> = OnceLock::new();
    I.get_or_init(|| {
        let mut views = HashMap::new();
        let mut routines: HashMap<String, Vec<Shape>> = HashMap::new();
        for s in parse(TEXT).into_iter().chain(parse(PLAN)) {
            let key = format!("{}.{}", s.schema, s.name);
            if s.kind == 'V' {
                views.insert(key, s);
            } else {
                routines.entry(key).or_default().push(s);
            }
        }
        Index { views, routines }
    })
}

/// The view `schema.name`.
#[must_use]
pub fn view(schema: &str, name: &str) -> Option<&'static Shape> {
    index().views.get(&format!("{schema}.{name}"))
}

/// The routines named `schema.name`, every overload.
#[must_use]
pub fn routines(schema: &str, name: &str) -> &'static [Shape] {
    index().routines.get(&format!("{schema}.{name}")).map_or(&[], Vec::as_slice)
}

/// The routine `schema.name` of `kind` (`T`, `S`, `P`), the first.
#[must_use]
pub fn routine(schema: &str, name: &str, kind: char) -> Option<&'static Shape> {
    routines(schema, name).iter().find(|s| s.kind == kind)
}

/// The routine with specific name `schema.specific`.
#[must_use]
pub fn index_specific(schema: &str, specific: &str) -> Option<&'static Shape> {
    index().routines.values().flatten().find(|s| s.schema == schema && s.specific == specific)
}
