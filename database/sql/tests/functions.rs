// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Scalar functions at the 7.5 level: each value and result type as
//! pub400 gave them for the same `VALUES` (tools/conformance/sql/cases/query/).

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::{Naming, Outcome, SqlContext, SqlError, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

struct Db {
    store: Store,
    ctx: SqlContext,
}

impl Db {
    fn new() -> Self {
        let d = Arc::new(MemBlockDev::with_seed(8192, 11));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 128,
            now: 1_767_225_600,
        };
        let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &params, StoreConfig::default()).unwrap();
        let (qsys, lib) = {
            let mut t = store.begin().unwrap();
            let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
            let lib = ops::create(
                &mut t,
                &CreateParams {
                    library: qsys,
                    name: Name::parse("QSTEST").unwrap(),
                    type_code: types::LIB,
                    attribute: String::new(),
                    text: "Test library".into(),
                    owner: "QSECOFR".into(),
                    system: "QS400".into(),
                    now: 1_767_225_600,
                    by_system: false,
                },
            )
            .unwrap();
            t.commit().unwrap();
            (qsys, lib)
        };
        let mut ctx = SqlContext::new(qsys, JobHandle(1), NameContext { curlib: Some(lib), libl: vec![lib] });
        ctx.now = Some(1_767_225_600);
        Self { store, ctx }
    }

    fn run(&mut self, text: &str) -> Result<Outcome, SqlError> {
        let p = prepare(text, Naming::Sql)?;
        let mut t = self.store.begin().unwrap();
        let r = p.execute(&mut t, &mut self.ctx, &[]);
        t.abort();
        r
    }

    /// The one value and the type of `VALUES (e)`.
    fn one(&mut self, e: &str) -> (String, String) {
        let text = format!("values ({e})");
        match self.run(&text) {
            Ok(Outcome::Rows(rs)) => {
                let v = &rs.rows[0][0];
                let c = &rs.columns[0];
                (
                    if v.is_null() { "-".into() } else { v.to_string() },
                    format!("{}{}", c.data_type.replace(' ', ""), if c.nullable { "" } else { " NN" }),
                )
            }
            o => panic!("{text}: {o:?}"),
        }
    }

    fn code(&mut self, e: &str) -> i32 {
        match self.run(&format!("values ({e})")) {
            Ok(o) => panic!("{e}: {o:?}"),
            Err(e) => e.sqlcode(),
        }
    }
}

/// Expression, value, type.
const CASES: &[(&str, &str, &str)] = &[
    ("timestamp_format('2024-03-05 14:07:09', 'YYYY-MM-DD HH24:MI:SS')", "2024-03-05 14:07:09.000000", "TIMESTAMP NN"),
    ("to_date('05/03/2024', 'DD/MM/YYYY')", "2024-03-05 00:00:00.000000", "TIMESTAMP NN"),
    ("timestamp_format('03-05-24 02:07:09 PM', 'MM-DD-YY HH:MI:SS AM')", "2024-03-05 14:07:09.000000", "TIMESTAMP NN"),
    ("timestamp_format('2024-065', 'YYYY-DDD')", "2024-03-05 00:00:00.000000", "TIMESTAMP NN"),
    ("timestamp_format('Mar 5 2024', 'Mon DD YYYY')", "2024-03-05 00:00:00.000000", "TIMESTAMP NN"),
    ("timestamp_format('2024-03-05 14:07:09.123456', 'YYYY-MM-DD HH24:MI:SS.FF6', 12)", "2024-03-05 14:07:09.123456000000", "TIMESTAMP(12) NN"),
    ("to_char(timestamp('2024-03-05-14.07.09'), 'DD.MM.YYYY HH:MI AM')", "05.03.2024 02:07 PM", "VARCHAR(255) NN"),
    ("varchar_format(timestamp('2024-03-05-14.07.09'), 'Day, Month DD, YYYY')", "Tuesday, March 05, 2024", "VARCHAR(255) NN"),
    (
        "varchar_format(timestamp('2024-03-05-14.07.09'), 'DAY MONTH DY MON D DDD J Q W WW IW I IY IYYY RR CC')",
        "TUESDAY MARCH TUE MAR 3 065 2460375 1 1 10 10 4 24 2024 24 21",
        "VARCHAR(255) NN",
    ),
    ("varchar_format(timestamp('2024-03-05-00.07.09'), 'HH12 HH24 MI SS SSSSS NNNNNN FF1 FF9')", "12 00 07 09 00429 000000 0 000000000", "VARCHAR(255) NN"),
    ("varchar_format(1234.5, '9,999.99')", " 1,234.50", "VARCHAR(254) NN"),
    ("varchar_format(-12.3, 'S0009.990')", "-0012.300", "VARCHAR(254) NN"),
    ("varchar_format(12, '999')", "  12", "VARCHAR(254) NN"),
    ("to_char(12345.678, 'L99G999D99')", "$ 12,345.68", "VARCHAR(254) NN"),
    ("extract(year from date('2024-03-05'))", "2024", "INTEGER NN"),
    ("extract(second from timestamp('2024-03-05-14.07.09.5'))", "9.500000", "DECIMAL(8,6) NN"),
    ("extract(epoch from timestamp('2024-03-05-14.07.09'))", "1709647629", "BIGINT NN"),
    ("extract(dow from date('2024-03-05'))", "3", "INTEGER NN"),
    ("extract(milliseconds from timestamp('2024-03-05-14.07.09.5'))", "9500", "INTEGER NN"),
    ("timestampdiff(16, char(timestamp('2024-03-05-14.07.09') - timestamp('2024-01-01-00.00.00')))", "64", "INTEGER NN"),
    ("timestampdiff_big(2, char(timestamp('2024-03-05-14.07.09') - timestamp('2024-01-01-00.00.00')))", "5580429", "BIGINT NN"),
    ("timestampdiff(64, '00000201000000.000000')", "2", "INTEGER NN"),
    ("timestampdiff(1, '00000000000001.000001')", "1000001", "INTEGER NN"),
    ("timestamp_iso(date('2024-03-05'))", "2024-03-05 00:00:00.000000", "TIMESTAMP NN"),
    ("round_timestamp(timestamp('2024-03-05-14.37.09'), 'HH')", "2024-03-05 15:00:00.000000", "TIMESTAMP NN"),
    ("trunc_timestamp(timestamp('2024-03-05-14.37.09'), 'MI')", "2024-03-05 14:37:00.000000", "TIMESTAMP NN"),
    ("round_timestamp(timestamp('2024-03-16-14.37.09'), 'MONTH')", "2024-04-01 00:00:00.000000", "TIMESTAMP NN"),
    ("round_timestamp(timestamp('2024-03-05-14.37.09'))", "2024-03-06 00:00:00.000000", "TIMESTAMP NN"),
    ("trunc_timestamp(timestamp('2024-03-07-14.37.09'), 'IW')", "2024-03-04 00:00:00.000000", "TIMESTAMP NN"),
    ("trunc_timestamp(timestamp('2024-03-07-14.37.09'), 'DAY')", "2024-03-03 00:00:00.000000", "TIMESTAMP NN"),
    ("trunc_timestamp(timestamp('2024-08-07-14.37.09'), 'Q')", "2024-07-01 00:00:00.000000", "TIMESTAMP NN"),
    ("round_timestamp(timestamp('2024-07-05-14.37.09'), 'YYYY')", "2025-01-01 00:00:00.000000", "TIMESTAMP NN"),
    ("round_timestamp(date('2024-03-17'), 'MONTH')", "2024-04-01 00:00:00", "TIMESTAMP(0) NN"),
    ("dayofweek_iso(date('2024-03-03'))", "7", "INTEGER NN"),
    ("week_iso(date('2021-01-01'))", "53", "INTEGER NN"),
    ("week_iso(date('2024-12-30'))", "1", "INTEGER NN"),
    ("translate('abcdef', 'xy', 'ab')", "xycdef", "VARCHAR(6) NN"),
    ("translate('abcdef')", "ABCDEF", "VARCHAR(6) NN"),
    ("translate('abcdef', 'x', 'abc', '*')", "x**def", "VARCHAR(6) NN"),
    ("overlay('abcdef' placing 'XY' from 3 for 2)", "abXYef", "VARCHAR(6) NN"),
    ("overlay('abcdef', 'XY', 3)", "abXYdef", "VARCHAR(7) NN"),
    ("insert('abcdef', 3, 2, 'XYZ')", "abXYZef", "VARCHAR(7) NN"),
    ("repeat('ab', 3)", "ababab", "VARCHAR(6) NN"),
    ("space(3)", "   ", "VARCHAR(3) NN"),
    ("lpad('abc', 6, 'xy')", "xyxabc", "VARCHAR(6) NN"),
    ("rpad('abc', 6)", "abc   ", "VARCHAR(6) NN"),
    ("instr('abcabc', 'bc', 1, 2)", "5", "INTEGER NN"),
    ("instr('abcabc', 'bc', -1)", "5", "INTEGER NN"),
    ("locate_in_string('abcabc', 'bc', -1, 2)", "2", "INTEGER NN"),
    ("regexp_count('banana', 'an')", "2", "INTEGER"),
    ("regexp_instr('banana', 'an', 1, 2)", "4", "INTEGER"),
    ("regexp_instr('banana', 'an', 1, 1, 1)", "4", "INTEGER"),
    ("regexp_substr('banana', 'a(n)', 1, 2, 'c', 1)", "n", "VARCHAR(6)"),
    ("regexp_substr('hello world', '\\w+$')", "world", "VARCHAR(11)"),
    ("regexp_replace('banana', 'an', 'AN')", "bANANa", "VARCHAR(20)"),
    ("regexp_replace('banana', 'an', 'X', 1, 2)", "banXa", "VARCHAR(13)"),
    ("regexp_replace('a1b22c', '\\d+', '#')", "a#b#c", "VARCHAR(13)"),
    ("regexp_replace('ABC', 'b', 'x', 1, 0, 'i')", "AxC", "VARCHAR(7)"),
    ("hex('AB')", "C1C2", "VARCHAR(4) NN"),
    ("bitand(12, 10)", "8", "INTEGER NN"),
    ("bitnot(cast(12 as smallint))", "-13", "SMALLINT NN"),
    ("bitxor(cast(12 as bigint), 10)", "6", "BIGINT NN"),
    ("bitandnot(12, 10)", "4", "INTEGER NN"),
    ("hash_md5('abc')", "FE4EA0D98F9CD8D1D27F102A93CB0BB0", "BINARY(16) NN"),
    ("hash_sha256('abc')", "B58EA6D31995A4D8CE092EB718DDFA58B6CEF2288B41FAF1DCD52FF3D6D8FA01", "BINARY(32) NN"),
    ("hash_sha1(x'616263')", "A9993E364706816ABA3E25717850C26C9CD0D89D", "BINARY(20) NN"),
    ("hash('abc')", "FE4EA0D98F9CD8D1D27F102A93CB0BB0", "VARBINARY(16) NN"),
    ("length(generate_unique())", "13", "INTEGER NN"),
    ("greatest(1, 5, 3)", "5", "INTEGER NN"),
    ("least('b', 'a', 'c')", "a", "VARCHAR(1) NN"),
    ("greatest(1, null, 3)", "-", "INTEGER"),
    ("nvl(null, 'x')", "x", "VARCHAR(1) NN"),
    ("interpret(bx'00000001' as integer)", "1", "INTEGER NN"),
    ("exp(1)", "2.718281828459045", "DOUBLE NN"),
    ("ln(10)", "2.302585092994046", "DOUBLE NN"),
    ("sinh(1)", "1.1752011936438014", "DOUBLE NN"),
    ("atanh(0.5)", "0.5493061443340549", "DOUBLE NN"),
    ("cot(1)", "0.6420926159343308", "DOUBLE NN"),
    ("sign(-3.2)", "-1.0", "DECIMAL(2,1) NN"),
    ("sign(cast(-3 as int))", "-1", "INTEGER NN"),
    ("radians(180)", "3.141592653589793", "DOUBLE NN"),
    ("degrees(pi())", "180", "DOUBLE NN"),
    ("mod(-7, 3)", "-1", "INTEGER NN"),
    ("mod(7.5, 2)", "1.5", "DECIMAL(2,1) NN"),
    ("ceiling(-1.5)", "-1", "DECIMAL(2,0) NN"),
    ("round(2.555, 2)", "2.560", "DECIMAL(5,3) NN"),
    ("truncate(-2.567, 1)", "-2.500", "DECIMAL(4,3) NN"),
    ("abs(-3)", "3", "INTEGER NN"),
    ("char(date('2024-03-05'), usa)", "03/05/2024", "CHAR(10) NN"),
    ("digits(cast(-12 as dec(5,2)))", "01200", "CHAR(5) NN"),
    ("zoned(12.5, 5, 2)", "12.50", "NUMERIC(5,2) NN"),
    ("ascii('A')", "65", "INTEGER NN"),
    ("chr(65)", "A", "CHAR(1)"),
    ("soundex('Robert')", "R163", "CHAR(4) NN"),
    ("difference('Robert','Rupert')", "4", "INTEGER NN"),
    ("months_between(date('2024-03-15'), date('2024-01-01'))", "2.451612903225806", "DECIMAL(31,15) NN"),
    ("quarter(date('2024-05-05'))", "2", "INTEGER NN"),
    ("midnight_seconds(time('01:00:01'))", "3601", "INTEGER NN"),
    ("julian_day(date('2024-03-05'))", "2460375", "INTEGER NN"),
    ("multiply_alt(12.34, 5.6)", "69.104", "DECIMAL(6,3) NN"),
    ("add_days(date('2024-03-05'), 3)", "2024-03-08", "DATE NN"),
    ("add_hours(timestamp('2024-03-05-14.07.09'), 3)", "2024-03-05 17:07:09.000000", "TIMESTAMP NN"),
    ("add_years(date('2024-02-29'), 1)", "2025-02-28", "DATE NN"),
    ("strip('xxabcxx', b, 'x')", "abc", "VARCHAR(7) NN"),
    ("ltrim('xxabc', 'x')", "abc", "VARCHAR(5) NN"),
    ("varchar_bit_format(x'C1C2')", "AB", "VARBINARY(1) NN"),
    ("varchar_format_binary(x'C1C2D3D4C1C2D3D4C1C2D3D4C1C2D3D4', 'xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx')", "c1c2d3d4-c1c2-d3d4-c1c2-d3d4c1c2d3d4", "VARCHAR(40) NN"),
    ("hextoraw('C1C2')", "C1C2", "VARBINARY(2) NN"),
    ("timestamp('20240305140709')", "2024-03-05 14:07:09.000000", "TIMESTAMP NN"),
    ("date('2024065')", "2024-03-05", "DATE NN"),
    ("date(738950)", "2024-03-05", "DATE NN"),
    ("date('2024-03-01') - cast(101 as dec(8,0))", "2024-01-29", "DATE NN"),
    ("date('2024-03-01') + cast(10203 as dec(8,0))", "2025-05-04", "DATE NN"),
    ("time('10:00:00') + cast(10203 as dec(6,0))", "11.02.03", "TIME NN"),
    ("current timezone", "0", "DECIMAL(6,0) NN"),
    ("current server", "QS400", "VARCHAR(18) NN"),
];

#[test]
fn scalar_functions_as_on_pub400() {
    let mut db = Db::new();
    let mut wrong = Vec::new();
    for (e, v, t) in CASES {
        let got = db.one(e);
        if got != ((*v).to_string(), (*t).to_string()) {
            wrong.push(format!("{e}: got {got:?}, want ({v:?}, {t:?})"));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[test]
fn refusals() {
    let mut db = Db::new();
    assert_eq!(db.code("asinh(1)"), -204);
    assert_eq!(db.code("date_trunc('MONTH', current date)"), -204);
    assert_eq!(db.code("encrypt_aes('secret', 'password1234')"), -84);
    assert_eq!(db.code("decrypt_char(x'00', 'password1234')"), -84);
    assert_eq!(db.code("timestamp_format('bad', 'YYYY')"), -302);
    assert_eq!(db.code("position('c' in 'abcabc' using octets)"), -199);
    assert_eq!(db.code("character_length('abc', octets)"), -206);
    assert_eq!(db.code("date('2024-03-01') + 2 weeks"), -104);
    assert_eq!(db.code("date('2024-03-01') - 101"), -182);
}
