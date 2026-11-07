// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test database's schemas and tables (see the module above for
//! what each is for).

use std::time::Duration;

use qap_proto::ops::sql::{SqlValue, TableKind};

use super::values::{Col, col, date_ms, parse_timestamp};

/// A table, view, alias or file.
#[derive(Debug, Clone)]
pub struct Table {
    /// Its schema.
    pub schema: String,
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: TableKind,
    /// Its description.
    pub text: String,
    /// Its columns; a view's, alias's or logical file's are its base's.
    pub columns: Vec<Col>,
    /// Its rows; empty for a view, alias or logical file.
    pub rows: Vec<Vec<SqlValue>>,
    /// The unique key column.
    pub key: Option<usize>,
    /// Journaled: changes may run under commitment control.
    pub journaled: bool,
    /// The only profile authorized to it; `None` for everyone.
    pub only: Option<&'static str>,
    /// Time each row takes to fetch (a query) or insert.
    pub slow: Option<Duration>,
    /// What a view, alias or logical file reads: the base table's
    /// name, and the column and value its rows must have.
    pub over: Option<(String, Option<(usize, SqlValue)>)>,
}

fn table(schema: &str, name: &str, text: &str, columns: Vec<Col>) -> Table {
    Table {
        schema: schema.into(),
        name: name.into(),
        kind: TableKind::Table,
        text: text.into(),
        columns,
        rows: Vec::new(),
        key: None,
        journaled: true,
        only: None,
        slow: None,
        over: None,
    }
}

fn over(
    schema: &str,
    name: &str,
    kind: TableKind,
    text: &str,
    base: &Table,
    filter: Option<(usize, SqlValue)>,
) -> Table {
    Table {
        kind,
        columns: base.columns.clone(),
        over: Some((base.name.clone(), filter)),
        journaled: false,
        ..table(schema, name, text, Vec::new())
    }
}

fn t(s: &str) -> SqlValue {
    SqlValue::Text { text: s.into() }
}

fn i(v: i64) -> SqlValue {
    SqlValue::Integer { value: v }
}

fn d(s: &str) -> SqlValue {
    SqlValue::Decimal { text: s.into() }
}

fn f(s: &str) -> SqlValue {
    SqlValue::Float { text: s.into() }
}

fn b(v: &[u8]) -> SqlValue {
    SqlValue::Binary { data: v.to_vec() }
}

fn day(y: i64, m: u32, dd: u32) -> SqlValue {
    SqlValue::Date {
        ms: date_ms(y, m, dd),
    }
}

fn ts(s: &str) -> SqlValue {
    let (ms, picos) = parse_timestamp(s).expect("a timestamp");
    SqlValue::Timestamp { ms, picos }
}

fn pad(s: &str, n: usize) -> SqlValue {
    t(&format!("{s:<n$}"))
}

/// The twelve assets in service, as Navigator's sample shows them.
const ASSETS: &[[&str; 8]] = &[
    [
        "QS000101",
        "QS-HQ-R1",
        "QUANTA",
        "T4048-IX8",
        "QTFCEV2340021",
        "SWITCH",
        "ACTIVE",
        "2025-03-11",
    ],
    [
        "QS000102",
        "QS-HQ-R1",
        "QUANTA",
        "T4048-IX8",
        "QTFCEV2340022",
        "SWITCH",
        "ACTIVE",
        "2025-03-11",
    ],
    [
        "QS000110",
        "QS-HQ-R1",
        "EDGECORE",
        "AS5712-54X",
        "571254X2041B0019",
        "SWITCH",
        "ACTIVE",
        "2024-11-02",
    ],
    [
        "QS000111",
        "QS-HQ-R1",
        "EDGECORE",
        "AS5712-54X",
        "571254X2041B0020",
        "SWITCH",
        "ACTIVE",
        "2024-11-02",
    ],
    [
        "QS000201",
        "QS-HQ-R2",
        "DELL",
        "R740XD",
        "7XK2Q33",
        "SERVER",
        "ACTIVE",
        "2024-06-20",
    ],
    [
        "QS000202",
        "QS-HQ-R2",
        "DELL",
        "R740XD",
        "7XK2Q34",
        "SERVER",
        "ACTIVE",
        "2024-06-20",
    ],
    [
        "QS000203",
        "QS-HQ-R2",
        "DELL",
        "R740XD",
        "7XK2Q35",
        "SERVER",
        "ACTIVE",
        "2024-06-20",
    ],
    [
        "QS000210",
        "QS-HQ-R2",
        "DELL",
        "QLE2562",
        "BFE1234567",
        "ADAPTER",
        "ACTIVE",
        "2026-09-20",
    ],
    [
        "QS000302",
        "QS-HQ-R3",
        "WEIDMULLER",
        "UR20-FBC-EIP",
        "2024-11-00341",
        "IO-COUPLER",
        "ACTIVE",
        "2026-10-01",
    ],
    [
        "QS000401",
        "QS-BR1-R1",
        "DELL",
        "S4048-ON",
        "8H3K2P2",
        "SWITCH",
        "ACTIVE",
        "2024-09-08",
    ],
    [
        "QS000402",
        "QS-BR1-R1",
        "WATCHGUARD",
        "M370",
        "80BA0F2B1C33",
        "FIREWALL",
        "ACTIVE",
        "2024-09-08",
    ],
    [
        "QS000403",
        "QS-BR1-R1",
        "ARISTA",
        "7050SX-64",
        "JPE14210033",
        "SWITCH",
        "SPARE",
        "2023-05-02",
    ],
];

/// `HLINV.ASSETS`: Navigator's columns and its twelve assets in
/// service.
fn assets() -> Table {
    let mut a = table(
        "HLINV",
        "ASSETS",
        "Hardware assets",
        vec![
            col("ASSET_TAG", "Asset tag", "CHAR(10)", false),
            col("LOCATION", "Rack / site code", "CHAR(10)", false),
            col("MANUFACTURER", "Manufacturer", "VARCHAR(40)", false),
            col("MODEL", "Model", "VARCHAR(40)", false),
            col("SERIAL", "Serial number", "VARCHAR(40)", true),
            col("ASSET_TYPE", "Type code", "CHAR(12)", false),
            col("STATUS", "Status", "CHAR(8)", false),
            col("ACQUIRED", "Acquisition date", "DATE", true),
            col("COST", "Purchase cost", "DECIMAL(11,2)", true),
            col("RACK_U", "Rack unit position", "SMALLINT", true),
            col("NOTES", "Short notes", "VARCHAR(256)", true),
            col("CHANGED_TS", "Last change", "TIMESTAMP(6)", false),
        ],
    );
    a.key = Some(0);
    for (n, r) in ASSETS.iter().enumerate() {
        let (y, m, dd) = (&r[7][..4], &r[7][5..7], &r[7][8..]);
        let n64 = i64::try_from(n).unwrap_or(0);
        a.rows.push(vec![
            pad(r[0], 10),
            pad(r[1], 10),
            t(r[2]),
            t(r[3]),
            t(r[4]),
            pad(r[5], 12),
            pad(r[6], 8),
            day(
                y.parse().unwrap_or(2026),
                m.parse().unwrap_or(1),
                dd.parse().unwrap_or(1),
            ),
            d(&format!("{}.00", 1200 + 150 * n64)),
            i(10 + n64),
            SqlValue::Null,
            ts("2026-10-29-13.58.02.000000"),
        ]);
    }
    a
}

/// `HLTEST.ALLTYPES`: one column of every type, seven rows; the
/// `DECFLOAT`s of rows 1 to 6 are `1234.5678`, `1.5E-30`, `Infinity`,
/// `-Infinity`, `NaN` and `sNaN`; row 7 is null but for its key.
fn all_types() -> Table {
    let mut a = table(
        "HLTEST",
        "ALLTYPES",
        "One column of every type",
        vec![
            col("ID", "Row", "INTEGER", false),
            col("C_SMALLINT", "SMALLINT", "SMALLINT", true),
            col("C_INTEGER", "INTEGER", "INTEGER", true),
            col("C_BIGINT", "BIGINT", "BIGINT", true),
            col("C_DECIMAL", "DECIMAL", "DECIMAL(11,2)", true),
            col("C_NUMERIC", "NUMERIC", "NUMERIC(7,3)", true),
            col("C_DECFLOAT", "DECFLOAT", "DECFLOAT(34)", true),
            col("C_REAL", "REAL", "REAL", true),
            col("C_DOUBLE", "DOUBLE", "DOUBLE", true),
            col("C_CHAR", "CHAR", "CHAR(10)", true),
            col("C_VARCHAR", "VARCHAR", "VARCHAR(40)", true),
            col("C_GRAPHIC", "GRAPHIC", "GRAPHIC(5)", true),
            col("C_VARGRAPHIC", "VARGRAPHIC", "VARGRAPHIC(20)", true),
            col("C_CLOB", "CLOB", "CLOB(65536)", true),
            col("C_BITS", "CHAR FOR BIT DATA", "CHAR(8) FOR BIT DATA", true),
            col("C_BINARY", "BINARY", "BINARY(4)", true),
            col("C_VARBINARY", "VARBINARY", "VARBINARY(16)", true),
            col("C_BLOB", "BLOB", "BLOB(65536)", true),
            col("C_DATE", "DATE", "DATE", true),
            col("C_TIME", "TIME", "TIME", true),
            col("C_TS", "TIMESTAMP", "TIMESTAMP(6)", true),
            col("C_TS12", "TIMESTAMP(12)", "TIMESTAMP(12)", true),
            col("C_BOOLEAN", "BOOLEAN", "BOOLEAN", true),
        ],
    );
    a.key = Some(0);
    let decfloats = [
        "1234.5678",
        "1.5E-30",
        "Infinity",
        "-Infinity",
        "NaN",
        "sNaN",
    ];
    for (m, df) in decfloats.iter().enumerate() {
        let k = i64::try_from(m).unwrap_or(0) + 1;
        // Five sets of the other values, the first again in row 6.
        let n = m % 5;
        a.rows.push(vec![
            i(k),
            i(-32768 + k),
            i(2_147_483_600 + k),
            i(-9_007_199_254_740_993 + k),
            d(&format!("{k}23456789.01")),
            d(&format!("-{k}.125")),
            d(df),
            f(["1.5", "3.4028235e38", "-0.1", "1e-45", "0"][n]),
            f(["0.1", "1e300", "-2.5e-308", "3.141592653589793", "-0"][n]),
            pad(["Alpha", "Beta", "Gamma", "Delta", ""][n], 10),
            t(["Grüße", "naïve café", "日本語", "", " lead"][n]),
            pad(["ＡＢ", "漢字", "", "Ω", "x"][n], 5),
            t(["全角テキスト", "", "ω", "a", "b"][n]),
            t(["A long text", "", "line\nbreak", "€", "z"][n]),
            b(&[
                [0x00, 0x01, 0xFE, 0xFF, 0x40, 0x40, 0x40, 0x40],
                [0xC1, 0xC2, 0xC3, 0x40, 0x40, 0x40, 0x40, 0x40],
                [0x40; 8],
                [0xFF; 8],
                [0x10; 8],
            ][n]),
            b(&[
                [1, 2, 3, 4],
                [0, 0, 0, 0],
                [0xFF; 4],
                [0xDE, 0xAD, 0xBE, 0xEF],
                [9, 9, 9, 9],
            ][n]),
            b([&[0x01, 0x02][..], &[], &[0xFF; 16], &[0x7F], &[0x80, 0x81]][n]),
            b([&b"%PDF-1.4"[..], &[], &[0; 300], &[1], &[2, 3]][n]),
            day(
                [2026, 1970, 1, 9999, 2000][n],
                [11, 1, 1, 12, 2][n],
                [2, 1, 1, 31, 29][n],
            ),
            SqlValue::Time {
                ms: [34_445_000, 0, 86_399_000, 43_200_000, 1_000][n],
            },
            ts([
                "2026-11-02-09.14.03.123456",
                "1970-01-01-00.00.00.000000",
                "0001-01-01-00.00.00.000000",
                "9999-12-31-23.59.59.999999",
                "2000-02-29-12.00.00.000001",
            ][n]),
            ts([
                "2026-11-02-09.14.03.123456789012",
                "1970-01-01-00.00.00.000000000001",
                "0001-01-01-00.00.00.000000000000",
                "9999-12-31-23.59.59.999999999999",
                "2000-02-29-12.00.00.500000000000",
            ][n]),
            SqlValue::Boolean { value: n % 2 == 0 },
        ]);
    }
    let mut nulls = vec![SqlValue::Null; a.columns.len()];
    nulls[0] = i(7);
    a.rows.push(nulls);
    a
}

fn id_name(schema: &str, name: &str, text: &str, journaled: bool) -> Table {
    let mut x = table(
        schema,
        name,
        text,
        vec![
            col("ID", "Identifier", "INTEGER", false),
            col("NAME", "Name", "VARCHAR(20)", true),
        ],
    );
    x.key = Some(0);
    x.journaled = journaled;
    x
}

/// Every schema and table.
#[must_use]
pub fn tables() -> (Vec<(String, String)>, Vec<Table>) {
    let mut schemas: Vec<(String, String)> = [
        ("HLINV", "Homelab inventory"),
        ("HLTEST", "QAP test tables"),
        ("HLPAY", "Payroll"),
        ("HLMANY", "Many tables"),
        ("QGPL", "General purpose library"),
        ("QSYS2", "System catalog"),
    ]
    .iter()
    .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
    .collect();
    for n in 1..=140 {
        schemas.push((format!("ARCH{n:03}"), format!("Archive {n}")));
    }
    let assets = assets();
    let active = over(
        "HLINV",
        "ASSETS_ACTIVE",
        TableKind::View,
        "Assets in service",
        &assets,
        Some((6, t("ACTIVE"))),
    );
    let alias = over(
        "HLINV",
        "HW",
        TableKind::Alias,
        "Hardware assets",
        &assets,
        None,
    );
    let logical = over(
        "HLINV",
        "ASSETSL1",
        TableKind::LogicalFile,
        "Assets by location",
        &assets,
        None,
    );
    let mut locations = table(
        "HLINV",
        "LOCATIONS",
        "Racks and sites",
        vec![
            col("LOCATION", "Rack / site code", "CHAR(10)", false),
            col("SITE", "Site", "VARCHAR(40)", false),
        ],
    );
    locations.key = Some(0);
    for (l, s) in [
        ("QS-HQ-R1", "Headquarters"),
        ("QS-HQ-R2", "Headquarters"),
        ("QS-HQ-R3", "Headquarters"),
        ("QS-BR1-R1", "Branch 1"),
    ] {
        locations.rows.push(vec![pad(l, 10), t(s)]);
    }
    let mut slow = id_name("HLTEST", "SLOW", "Rows that take 10 ms each to fetch", true);
    slow.slow = Some(Duration::from_millis(10));
    slow.rows = (1..=300)
        .map(|n| vec![i(n), t(&format!("Row {n}"))])
        .collect();
    let mut slowlog = id_name(
        "HLTEST",
        "SLOWLOG",
        "Rows that take 10 ms each to insert",
        true,
    );
    slowlog.slow = Some(Duration::from_millis(10));
    let mut big = table(
        "HLTEST",
        "BIG",
        "Over a megabyte of rows",
        vec![
            col("ID", "Identifier", "INTEGER", false),
            col("DATA", "Data", "VARCHAR(1000)", false),
        ],
    );
    big.rows = (1..=3000)
        .map(|n| vec![i(n), t(&format!("{n:0>1000}"))])
        .collect();
    let mut salaries = table(
        "HLPAY",
        "SALARIES",
        "Salaries",
        vec![
            col("EMPNO", "Employee", "CHAR(6)", false),
            col("SALARY", "Salary", "DECIMAL(9,2)", false),
        ],
    );
    salaries.only = Some("ANN");
    salaries.rows = vec![
        vec![pad("000010", 6), d("152750.00")],
        vec![pad("000020", 6), d("94250.00")],
    ];
    let mut qclsrc = table(
        "QGPL",
        "QCLSRC",
        "CL source",
        vec![
            col("SRCSEQ", "Sequence", "NUMERIC(6,2)", false),
            col("SRCDAT", "Date", "NUMERIC(6,0)", false),
            col("SRCDTA", "Source", "CHAR(80)", false),
        ],
    );
    qclsrc.kind = TableKind::PhysicalFile;
    qclsrc.journaled = false;
    let mut all = vec![
        assets,
        locations,
        active,
        alias,
        logical,
        all_types(),
        id_name("HLTEST", "JRN", "Journaled", true),
        id_name("HLTEST", "NOJRN", "Not journaled", false),
        slow,
        slowlog,
        big,
        salaries,
        qclsrc,
    ];
    for n in 1..=150 {
        all.push(table(
            "HLMANY",
            &format!("T{n:03}"),
            &format!("Table {n}"),
            vec![col("ID", "Identifier", "INTEGER", false)],
        ));
    }
    (schemas, all)
}
