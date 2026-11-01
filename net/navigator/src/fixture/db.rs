// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample database: schema HLINV and its tables, the ASSETS table's 412
//! rows, and a small statement runner for them (`sql`): SELECT with a
//! column list and WHERE, UPDATE and DELETE. It keeps rows in arrival
//! order (ORDER BY is not applied) and runs no other statement.

mod sql;

use std::fmt::Write as _;
use std::sync::Mutex;

use qsweb::msg::{Kind, Message};

use super::text;
use crate::env::db::{
    Column, Constraint, Db, DbObject, Index, ObjKind, Schema, SqlOptions, SqlOutcome, TableDesc, Trigger,
};
use crate::util::secs;

/// A sample table: its catalog entry, columns and rows.
#[derive(Clone, Debug)]
pub struct Table {
    /// The catalog entry.
    pub obj: DbObject,
    /// The columns.
    pub columns: Vec<Column>,
    /// The rows, in arrival order.
    pub rows: Vec<Vec<Option<String>>>,
}

/// The sample system's database.
pub struct FixtureDb {
    /// HLINV's tables, changeable by tests and statements.
    pub tables: Mutex<Vec<Table>>,
    /// Statements run, as given.
    pub ran: Mutex<Vec<String>>,
}

impl Default for FixtureDb {
    fn default() -> Self {
        Self {
            tables: Mutex::new(tables()),
            ran: Mutex::new(Vec::new()),
        }
    }
}

/// Bytes of `tenths` of a megabyte.
fn mb10(tenths: u64) -> u64 {
    tenths * (1 << 20) / 10
}

fn obj(name: &str, text: &str, rows: u64, size10: u64, changed: u64) -> DbObject {
    DbObject {
        name: name.into(),
        text: text.into(),
        rows,
        size: mb10(size10),
        changed,
        journaled: true,
        over: String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn col(name: &str, sys: &str, typ: &str, length: u32, scale: Option<u32>, nullable: bool, default: &str, text: &str) -> Column {
    Column {
        name: name.into(),
        system_name: sys.into(),
        typ: typ.into(),
        length,
        scale,
        nullable,
        default: default.into(),
        text: text.into(),
    }
}

/// ASSETS' columns.
fn asset_columns() -> Vec<Column> {
    vec![
        col("ASSET_TAG", "ASSETTAG", "CHAR", 10, None, false, "", "Asset tag (primary key)"),
        col("LOCATION", "LOCATION", "CHAR", 10, None, false, "", "Rack / site code"),
        col("MANUFACTURER", "MFR", "VARCHAR", 40, None, false, "", "Manufacturer"),
        col("MODEL", "MODEL", "VARCHAR", 40, None, false, "", "Model"),
        col("SERIAL", "SERIAL", "VARCHAR", 40, None, true, "NULL", "Serial number"),
        col("ASSET_TYPE", "ASSETTYPE", "CHAR", 12, None, false, "'OTHER'", "Type code"),
        col("STATUS", "STATUS", "CHAR", 8, None, false, "'ACTIVE'", "ACTIVE / SPARE / RETIRED"),
        col("ACQUIRED", "ACQUIRED", "DATE", 4, None, true, "NULL", "Acquisition date"),
        col("COST", "COST", "DECIMAL", 11, Some(2), true, "NULL", "Purchase cost"),
        col("RACK_U", "RACKU", "SMALLINT", 2, None, true, "NULL", "Rack unit position"),
        col("NOTES", "NOTES", "VARCHAR", 256, None, true, "NULL", "Short notes"),
        col("CHANGED_TS", "CHANGEDTS", "TIMESTAMP", 10, None, false, "CURRENT TIMESTAMP", "Last change"),
    ]
}

/// The twelve assets in service, as the prototype shows them.
const IN_SERVICE: &[[&str; 8]] = &[
    ["QS000101", "QS-HQ-R1", "QUANTA", "T4048-IX8", "QTFCEV2340021", "SWITCH", "ACTIVE", "2025-03-11"],
    ["QS000102", "QS-HQ-R1", "QUANTA", "T4048-IX8", "QTFCEV2340022", "SWITCH", "ACTIVE", "2025-03-11"],
    ["QS000110", "QS-HQ-R1", "EDGECORE", "AS5712-54X", "571254X2041B0019", "SWITCH", "ACTIVE", "2024-11-02"],
    ["QS000111", "QS-HQ-R1", "EDGECORE", "AS5712-54X", "571254X2041B0020", "SWITCH", "ACTIVE", "2024-11-02"],
    ["QS000201", "QS-HQ-R2", "DELL", "R740XD", "7XK2Q33", "SERVER", "ACTIVE", "2024-06-20"],
    ["QS000202", "QS-HQ-R2", "DELL", "R740XD", "7XK2Q34", "SERVER", "ACTIVE", "2024-06-20"],
    ["QS000203", "QS-HQ-R2", "DELL", "R740XD", "7XK2Q35", "SERVER", "ACTIVE", "2024-06-20"],
    ["QS000210", "QS-HQ-R2", "DELL", "QLE2562", "BFE1234567", "ADAPTER", "ACTIVE", "2026-09-20"],
    ["QS000302", "QS-HQ-R3", "WEIDMULLER", "UR20-FBC-EIP", "2024-11-00341", "IO-COUPLER", "ACTIVE", "2026-10-01"],
    ["QS000401", "QS-BR1-R1", "DELL", "S4048-ON", "8H3K2P2", "SWITCH", "ACTIVE", "2024-09-08"],
    ["QS000402", "QS-BR1-R1", "WATCHGUARD", "M370", "80BA0F2B1C33", "FIREWALL", "ACTIVE", "2024-09-08"],
    ["QS000403", "QS-BR1-R1", "ARISTA", "7050SX-64", "JPE14210033", "SWITCH", "SPARE", "2023-05-02"],
];

/// ASSETS' 412 rows: the twelve in service, then 400 retired ones.
pub fn asset_rows() -> Vec<Vec<Option<String>>> {
    const SITES: [&str; 4] = ["QS-HQ-R1", "QS-HQ-R2", "QS-HQ-R3", "QS-BR1-R1"];
    const KINDS: [(&str, &str, &str); 4] = [
        ("DELL", "R630", "SERVER"),
        ("QUANTA", "LB6M", "SWITCH"),
        ("APC", "SMT1500", "UPS"),
        ("SUPERMICRO", "X10SLM", "SERVER"),
    ];
    let some = |s: &str| Some(s.to_string());
    let mut rows = Vec::new();
    for (i, a) in IN_SERVICE.iter().enumerate() {
        let mut r: Vec<Option<String>> = a.iter().map(|v| some(v)).collect();
        r.push(some(&format!("{}.00", 1200 + 150 * i)));
        r.push(some(&(10 + i).to_string()));
        r.push(None);
        r.push(some("2026-10-29-13.58.02.000000"));
        rows.push(r);
    }
    for i in 0..400usize {
        let (mfr, model, kind) = KINDS[i % KINDS.len()];
        rows.push(vec![
            some(&format!("QS{:06}", 1001 + i)),
            some(SITES[i % SITES.len()]),
            some(mfr),
            some(model),
            some(&format!("RT{:08}", 5_000_000 + i * 7)),
            some(kind),
            some("RETIRED"),
            some(&format!("20{:02}-{:02}-{:02}", 15 + i % 6, 1 + i % 12, 1 + i % 28)),
            some(&format!("{}.00", 300 + (i % 40) * 25)),
            None,
            some("Retired"),
            some("2026-09-14-12.11.00.000000"),
        ]);
    }
    rows
}

/// HLINV's tables.
fn tables() -> Vec<Table> {
    let at = |m, d, h, mi| secs(2026, m, d, h, mi, 0);
    let simple = |cols: &[(&str, &str, u32, &str)]| {
        cols.iter()
            .map(|(n, t, l, x)| col(n, n, t, *l, None, false, "", x))
            .collect::<Vec<_>>()
    };
    let rows = asset_rows();
    vec![
        Table {
            obj: obj("ASSETS", "Hardware assets", rows.len() as u64, 31, at(10, 29, 13, 58)),
            columns: asset_columns(),
            rows,
        },
        Table {
            obj: obj("LOCATIONS", "Racks and sites", 14, 1, at(9, 20, 10, 0)),
            columns: simple(&[("LOCATION", "CHAR", 10, "Rack / site code"), ("SITE", "VARCHAR", 40, "Site")]),
            rows: Vec::new(),
        },
        Table {
            obj: obj("MANUFACTURERS", "Vendors", 38, 1, at(9, 14, 12, 11)),
            columns: simple(&[("MANUFACTURER", "VARCHAR", 40, "Manufacturer"), ("SUPPORT", "VARCHAR", 80, "Support contact")]),
            rows: Vec::new(),
        },
        Table {
            obj: obj("MOVEMENTS", "Asset movement log", 2_207, 94, at(10, 29, 6, 14)),
            columns: simple(&[("ASSET_TAG", "CHAR", 10, "Asset tag"), ("FROM_LOC", "CHAR", 10, "From"), ("TO_LOC", "CHAR", 10, "To")]),
            rows: Vec::new(),
        },
        Table {
            obj: obj("CONTRACTS", "Support contracts", 27, 2, at(10, 2, 15, 30)),
            columns: simple(&[("CONTRACT", "CHAR", 12, "Contract number"), ("MANUFACTURER", "VARCHAR", 40, "Vendor")]),
            rows: Vec::new(),
        },
        Table {
            obj: obj("NOTES", "Free text notes", 1_118, 740, at(10, 28, 17, 2)),
            columns: simple(&[("ASSET_TAG", "CHAR", 10, "Asset tag"), ("NOTE", "VARCHAR", 4000, "Note")]),
            rows: Vec::new(),
        },
    ]
}

/// The schemas.
fn schemas() -> Vec<Schema> {
    let s = |name: &str, text: &str, owner: &str, journal: &str, size10: u64, m: u32, d: u32| Schema {
        name: name.into(),
        text: text.into(),
        owner: owner.into(),
        journal: journal.into(),
        size: mb10(size10),
        created: secs(2026, m, d, 9, 0, 0),
    };
    vec![
        s("HLINV", "Homelab inventory", "CWELLMAN", "HLINV/QSQJRN", 881, 9, 14),
        s("PAYROLL", "Payroll", "QSECOFR", "PAYROLL/QSQJRN", 412, 3, 2),
        s("QGPL", "General purpose library", "QSYS", "", 1_204, 1, 1),
        s("QSYS2", "System catalog", "QSYS", "", 3_310, 1, 1),
        s("QUSRSYS", "System user objects", "QSYS", "", 960, 1, 1),
        s("WEBSTORE", "Web store", "WEBADM", "WEBSTORE/QSQJRN", 2_150, 6, 30),
    ]
}

/// HLINV's views, indexes and procedures.
fn others(kind: ObjKind) -> Vec<DbObject> {
    let at = secs(2026, 9, 14, 12, 11, 0);
    let o = |name: &str, text: &str, over: &str| DbObject {
        name: name.into(),
        text: text.into(),
        changed: at,
        journaled: false,
        over: over.into(),
        ..DbObject::default()
    };
    match kind {
        ObjKind::Tables => Vec::new(),
        ObjKind::Views => vec![
            o("ASSETS_ACTIVE", "Assets in service", "ASSETS"),
            o("ASSETS_BY_SITE", "Assets per site", "ASSETS"),
            o("CONTRACTS_DUE", "Contracts due in 90 days", "CONTRACTS"),
        ],
        ObjKind::Indexes => vec![
            o("ASSETS_IX1", "Assets by location", "ASSETS"),
            o("ASSETS_IX2", "Assets by model", "ASSETS"),
            o("ASSETS_SERIAL", "Assets by serial", "ASSETS"),
            o("LOCATIONS_IX1", "Locations by site", "LOCATIONS"),
            o("MFR_IX1", "Vendors by name", "MANUFACTURERS"),
            o("MOVEMENTS_IX1", "Movements by asset", "MOVEMENTS"),
            o("MOVEMENTS_IX2", "Movements by date", "MOVEMENTS"),
            o("CONTRACTS_IX1", "Contracts by vendor", "CONTRACTS"),
            o("NOTES_IX1", "Notes by asset", "NOTES"),
        ],
        ObjKind::Procedures => vec![
            o("MOVE_ASSET", "Move an asset to a location", ""),
            o("RETIRE_ASSET", "Retire an asset", ""),
        ],
    }
}

/// Message `id` from the fixture's texts.
pub fn msg(kind: Kind, id: &str, data: &[&str]) -> Message {
    let t = text::second_level(id, data).map(|s| s.text).unwrap_or_default();
    Message::new(kind, id, &t).with_data(data)
}

impl FixtureDb {
    fn find(&self, schema: &str, table: &str) -> Option<Table> {
        if schema != "HLINV" {
            return None;
        }
        self.tables.lock().expect("tables").iter().find(|t| t.obj.name == table).cloned()
    }
}

impl Db for FixtureDb {
    fn schemas(&self, _user: &str) -> Vec<Schema> {
        schemas()
    }

    fn sql_schemas(&self, _user: &str) -> Vec<String> {
        ["HLINV", "QGPL", "QSYS2", "WEBSTORE", "PAYROLL"].iter().map(ToString::to_string).collect()
    }

    fn objects(&self, _user: &str, schema: &str, kind: ObjKind) -> Vec<DbObject> {
        if schema != "HLINV" {
            return Vec::new();
        }
        match kind {
            ObjKind::Tables => self.tables.lock().expect("tables").iter().map(|t| t.obj.clone()).collect(),
            k => others(k),
        }
    }

    fn table(&self, _user: &str, schema: &str, table: &str) -> Option<TableDesc> {
        let t = self.find(schema, table)?;
        let assets = table == "ASSETS";
        let key = t.columns.first().map(|c| c.name.clone()).unwrap_or_default();
        let mut constraints = vec![Constraint {
            name: format!("{table}_PK"),
            kind: "PRIMARY KEY".into(),
            columns: key,
            references: String::new(),
        }];
        let mut indexes = Vec::new();
        let mut triggers = Vec::new();
        if assets {
            constraints.push(Constraint {
                name: "ASSETS_LOC_FK".into(),
                kind: "FOREIGN KEY".into(),
                columns: "LOCATION".into(),
                references: "HLINV.LOCATIONS (LOCATION)".into(),
            });
            constraints.push(Constraint {
                name: "ASSETS_STATUS_CK".into(),
                kind: "CHECK".into(),
                columns: "STATUS IN ('ACTIVE', 'SPARE', 'RETIRED')".into(),
                references: String::new(),
            });
            let ix = |name: &str, cols: &str, unique: bool, text: &str| Index {
                name: name.into(),
                schema: "HLINV".into(),
                columns: cols.into(),
                unique,
                text: text.into(),
            };
            indexes = vec![
                ix("ASSETS_IX1", "LOCATION, ASSET_TAG", false, "Assets by location"),
                ix("ASSETS_IX2", "MANUFACTURER, MODEL", false, "Assets by model"),
                ix("ASSETS_SERIAL", "SERIAL", true, "Assets by serial"),
            ];
            triggers = vec![Trigger {
                name: "ASSETS_CHG_TRG".into(),
                event: "UPDATE".into(),
                time: "BEFORE".into(),
                program: "HLINV/ASSETSTRG".into(),
                enabled: true,
            }];
        }
        Some(TableDesc {
            schema: schema.into(),
            system_name: t.obj.name.chars().take(10).collect(),
            owner: "CWELLMAN".into(),
            created: secs(2026, 9, 14, 9, 30, 0),
            format: t.obj.name.chars().take(9).collect::<String>() + "R",
            deleted: if assets { 3 } else { 0 },
            journal: "HLINV/QSQJRN".into(),
            table: t.obj,
            columns: t.columns,
            constraints,
            indexes,
            triggers,
        })
    }

    fn ddl(&self, user: &str, schema: &str, table: &str) -> Result<String, Message> {
        let Some(t) = self.table(user, schema, table) else {
            return Err(msg(Kind::Error, "SQL0204", &[table, schema]));
        };
        let cols: Vec<String> = t
            .columns
            .iter()
            .map(|c| {
                let mut s = format!("  {} FOR COLUMN {} {}", c.name, c.system_name, c.typ);
                let _ = match (c.typ.as_str(), c.scale) {
                    (_, Some(sc)) => write!(s, "({}, {sc})", c.length),
                    ("CHAR" | "VARCHAR", None) => write!(s, "({})", c.length),
                    _ => Ok(()),
                };
                if !c.nullable {
                    s.push_str(" NOT NULL");
                }
                if !c.default.is_empty() && c.default != "NULL" {
                    let _ = write!(s, " DEFAULT {}", c.default);
                }
                s
            })
            .collect();
        let mut out = format!("CREATE TABLE {schema}.{table} (\n{}", cols.join(",\n"));
        for c in &t.constraints {
            let body = match c.kind.as_str() {
                "CHECK" => format!("CHECK ({})", c.columns),
                "FOREIGN KEY" => format!("FOREIGN KEY ({}) REFERENCES {}", c.columns, c.references),
                k => format!("{k} ({})", c.columns),
            };
            let _ = write!(out, ",\n  CONSTRAINT {schema}.{} {body}", c.name);
        }
        let _ = write!(out, ");\n\nLABEL ON TABLE {schema}.{table} IS '{}';\n", t.table.text.replace('\'', "''"));
        Ok(out)
    }

    fn run_sql(&self, user: &str, statement: &str, opts: &SqlOptions, max_rows: Option<usize>) -> SqlOutcome {
        self.ran.lock().expect("ran").push(statement.to_string());
        let mut tables = self.tables.lock().expect("tables");
        let mut out = sql::run(&mut tables, user, statement, opts, max_rows);
        out.elapsed_ms = 41;
        out
    }

    fn explain(&self, _user: &str, statement: &str, opts: &SqlOptions) -> Result<Vec<String>, Message> {
        let tables = self.tables.lock().expect("tables");
        sql::explain(&tables, statement, opts)
    }
}
