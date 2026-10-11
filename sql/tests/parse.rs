// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parser over the dialect subset: a corpus that must parse, the
//! shapes of the statements, both naming modes, and the errors.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use sql::ast::{
    AlterAction, BinOp, CmpOp, ColumnDefault, Constraint, DataType, Expr, FetchOrientation,
    InsertSource, Isolation, JoinKind, Literal, MergeAction, Naming, QueryBody, Register,
    SchemaTarget, SelectItem, Statement, TableRef, Where, columns_of,
};
use sql::{SqlError, parse, parse_all};

fn sys(text: &str) -> Statement {
    parse(text, Naming::Sys).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn sqln(text: &str) -> Statement {
    parse(text, Naming::Sql).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn select(s: &Statement) -> &sql::ast::Select {
    match s {
        Statement::Select(q) => match &q.body {
            QueryBody::Select(sel) => sel,
            b => panic!("{b:?}"),
        },
        s => panic!("{s:?}"),
    }
}

#[test]
fn corpus_parses() {
    let corpus = [
        "SELECT * FROM QSTEST/CUST",
        "select cusno, name as n, bal * 1.1 from qstest/cust where bal > 100 and name like 'A%' order by 2 desc, cusno fetch first 10 rows only",
        "SELECT DISTINCT a.cusno, COUNT(*), SUM(o.amount) FROM cust a INNER JOIN orders o ON a.cusno = o.cusno GROUP BY a.cusno HAVING COUNT(*) > 1",
        "SELECT * FROM cust c LEFT OUTER JOIN orders o ON c.cusno = o.cusno WHERE o.cusno IS NULL",
        "SELECT * FROM cust EXCEPTION JOIN orders ON cust.cusno = orders.cusno",
        "SELECT * FROM cust c RIGHT EXCEPTION JOIN orders o ON c.cusno = o.cusno",
        "SELECT c.*, (SELECT MAX(amount) FROM orders WHERE cusno = c.cusno) FROM cust c",
        "SELECT * FROM cust WHERE cusno IN (SELECT cusno FROM orders) AND NOT EXISTS (SELECT 1 FROM bad WHERE bad.cusno = cust.cusno)",
        "SELECT * FROM cust WHERE cusno IN (1, 2, 3) OR cusno NOT BETWEEN 10 AND 20",
        "SELECT CASE WHEN bal < 0 THEN 'DEBIT' WHEN bal = 0 THEN 'ZERO' ELSE 'CREDIT' END, CASE cusno WHEN 1 THEN 'one' END FROM cust",
        "SELECT CAST(bal AS DECIMAL(15, 2)), CAST(name AS VARCHAR(10)), CAST(since AS CHAR(10)) FROM cust",
        "SELECT CURRENT DATE, CURRENT_TIMESTAMP, CURRENT USER, USER, CURRENT SCHEMA FROM sysibm/sysdummy1",
        "SELECT since + 3 DAYS, since - 1 MONTH, DAYS(since) - DAYS(CURRENT DATE) FROM cust",
        "SELECT SUBSTR(name, 1, 3), SUBSTRING(name FROM 2 FOR 4), TRIM(name), POSITION('a' IN name), name || ' ' CONCAT 'x' FROM cust",
        "SELECT RRN(cust), HEX(cusno), COALESCE(note, 'none'), VALUE(note, ''), IFNULL(note, '') FROM cust",
        "SELECT * FROM cust WHERE bal > ALL (SELECT bal FROM cust WHERE cusno < 3)",
        "SELECT * FROM cust WHERE (cusno, name) = (1, 'Alice')",
        "SELECT * FROM cust ORDER BY name NULLS LAST, cusno OFFSET 5 ROWS FETCH NEXT 5 ROWS ONLY",
        "SELECT * FROM cust LIMIT 10 OFFSET 20",
        "SELECT * FROM cust WHERE cusno = ? AND name = :NAME:IND WITH UR",
        "SELECT * FROM cust FOR UPDATE OF bal WITH CS",
        "SELECT * FROM cust FOR READ ONLY OPTIMIZE FOR 100 ROWS",
        "SELECT cusno FROM cust UNION ALL SELECT cusno FROM arch UNION SELECT 0 FROM sysibm/sysdummy1 ORDER BY 1",
        "VALUES (1, 'a'), (2, 'b')",
        "SELECT * FROM (SELECT cusno, bal FROM cust) AS t (c, b) WHERE b > 0",
        "SELECT X'C1C2', 1E3, -1.5, +2, 'it''s' FROM sysibm/sysdummy1",
        "SELECT * FROM cust WHERE name LIKE 'A\\_%' ESCAPE '\\'",
        "SELECT DATE '2026-01-01', TIME '12.00.00', TIMESTAMP '2026-01-01 12:00:00' FROM sysibm/sysdummy1",
        "INSERT INTO qstest/cust (cusno, name) VALUES (1, 'Alice'), (2, 'Bob')",
        "INSERT INTO cust VALUES (1, 'Alice', 0, NULL, DEFAULT)",
        "INSERT INTO arch SELECT * FROM cust WHERE bal = 0 WITH NC",
        "INSERT INTO cust DEFAULT VALUES",
        "UPDATE cust SET bal = bal * 1.05, note = NULL WHERE cusno = 1",
        "UPDATE cust AS c SET (bal, note) = (0, DEFAULT) WHERE c.cusno = 2",
        "UPDATE cust SET bal = 0 WHERE CURRENT OF c1",
        "DELETE FROM cust WHERE bal < 0",
        "DELETE FROM cust c WHERE CURRENT OF c1 WITH RS",
        "MERGE INTO cust c USING (SELECT * FROM newcust) n ON c.cusno = n.cusno WHEN MATCHED AND n.bal < 0 THEN DELETE WHEN MATCHED THEN UPDATE SET bal = n.bal WHEN NOT MATCHED THEN INSERT (cusno, name) VALUES (n.cusno, n.name)",
        "CREATE TABLE qstest/cust (cusno DECIMAL(7, 0) NOT NULL PRIMARY KEY, name CHAR(30) NOT NULL WITH DEFAULT, bal NUMERIC(9, 2) DEFAULT 0, since DATE, note VARCHAR(100) CCSID 37, id INTEGER GENERATED ALWAYS AS IDENTITY (START WITH 100, INCREMENT BY 10), CONSTRAINT uq UNIQUE (name), FOREIGN KEY (cusno) REFERENCES other (id) ON DELETE CASCADE, CHECK (bal >= 0)) RCDFMT cusrec",
        "CREATE TABLE t FOR SYSTEM NAME tsys (c1 FOR COLUMN c1sys SMALLINT, c2 BIGINT, c3 REAL, c4 DOUBLE PRECISION, c5 FLOAT(53), c6 TIME, c7 TIMESTAMP(6), c8 GRAPHIC(10), c9 VARGRAPHIC(20), c10 CHARACTER VARYING(5) FOR BIT DATA)",
        "CREATE TABLE copy AS (SELECT * FROM cust) WITH DATA",
        "ALTER TABLE cust ADD COLUMN phone CHAR(15) DROP COLUMN note CASCADE ALTER COLUMN name SET DATA TYPE CHAR(40) ALTER COLUMN bal SET NOT NULL ADD CONSTRAINT pk PRIMARY KEY (cusno) DROP CONSTRAINT uq",
        "DROP TABLE qstest/cust CASCADE",
        "CREATE UNIQUE WHERE NOT NULL INDEX cusbyname ON cust (name DESC, cusno) RCDFMT cusrec",
        "CREATE INDEX ix ON cust (bal)",
        "DROP INDEX ix",
        "CREATE OR REPLACE VIEW active (c, n) AS SELECT cusno, name FROM cust WHERE bal > 0 WITH CHECK OPTION",
        "DROP VIEW active",
        "CREATE ALIAS archcust FOR qstest/cust (arch)",
        "DROP ALIAS archcust",
        "LABEL ON TABLE cust IS 'Customers'",
        "LABEL ON COLUMN cust.name IS 'Customer name'",
        "LABEL ON COLUMN cust.name TEXT IS 'Name'",
        "LABEL ON TABLE cust (cusno IS 'Number', name TEXT IS 'Name')",
        "COMMENT ON TABLE cust IS 'The customers'",
        "COMMENT ON COLUMN cust.bal IS 'Balance'",
        "RENAME TABLE cust TO customers",
        "RENAME TABLE customers TO cust FOR SYSTEM NAME cust",
        "RENAME INDEX ix TO ix2",
        "DECLARE c1 CURSOR WITH HOLD FOR SELECT * FROM cust FOR UPDATE",
        "DECLARE c2 SCROLL CURSOR FOR SELECT cusno FROM cust ORDER BY cusno",
        "OPEN c1",
        "OPEN c2 USING :A, :B",
        "FETCH c1 INTO :CUSNO, :NAME:IND",
        "FETCH RELATIVE 2 FROM c2",
        "FETCH PRIOR FROM c2 INTO :X",
        "CLOSE c1",
        "COMMIT",
        "COMMIT WORK HOLD",
        "ROLLBACK HOLD",
        "SET SCHEMA qstest",
        "SET CURRENT SCHEMA = 'QSTEST'",
        "SET SCHEMA USER",
        "SET PATH \"QSTEST\", SYSTEM PATH, USER",
        "SET PATH = *LIBL",
        "SET CURRENT DEGREE = ANY",
        "EXPLAIN SELECT * FROM cust",
        "(SELECT 1 FROM sysibm/sysdummy1) UNION (SELECT 2 FROM sysibm/sysdummy1)",
    ];
    for text in corpus {
        sys(text);
    }
    let script = parse_all(
        "CREATE TABLE a (x INT); INSERT INTO a VALUES (1);; SELECT * FROM a;",
        Naming::Sys,
    )
    .unwrap();
    assert_eq!(script.len(), 3);
}

#[test]
fn shapes() {
    // A select with its clauses.
    let s = sys(
        "select cusno, name as n, bal * 1.1 from qstest/cust c where bal > 100 and name like 'A%' order by 2 desc, cusno fetch first 10 rows only",
    );
    let Statement::Select(q) = &s else { panic!() };
    let sel = select(&s);
    assert_eq!(sel.items.len(), 3);
    assert!(
        matches!(&sel.items[1], SelectItem::Expr(Expr::Column(c), Some(a)) if c.column.value == "NAME" && a.value == "N")
    );
    assert!(matches!(
        &sel.items[2],
        SelectItem::Expr(Expr::Binary(_, BinOp::Mul, _), None)
    ));
    match &sel.from[0] {
        TableRef::Table { name, alias, .. } => {
            assert_eq!(name.schema.as_ref().unwrap().value, "QSTEST");
            assert_eq!(name.name.value, "CUST");
            assert_eq!(alias.as_ref().unwrap().value, "C");
        }
        t => panic!("{t:?}"),
    }
    let Some(Expr::And(l, r)) = &sel.where_clause else {
        panic!()
    };
    assert!(matches!(**l, Expr::Compare(_, CmpOp::Gt, _, None)));
    assert!(matches!(**r, Expr::Like(_, _, None, false)));
    assert_eq!(q.order_by.len(), 2);
    assert!(q.order_by[0].desc);
    assert_eq!(q.order_by[0].expr, Expr::num("2"));
    assert_eq!(q.fetch_first, Some(Expr::num("10")));
    // Precedence: a + b * c || d, NOT binds tighter than AND, OR lowest.
    let s = sys("SELECT a + b * c || d FROM t WHERE NOT x = 1 AND y = 2 OR z = 3");
    let sel = select(&s);
    let SelectItem::Expr(e, _) = &sel.items[0] else {
        panic!()
    };
    let Expr::Binary(a, BinOp::Add, rest) = e else {
        panic!("{e:?}")
    };
    assert_eq!(**a, Expr::column("A"));
    let Expr::Binary(bc, BinOp::Concat, d) = &**rest else {
        panic!("{rest:?}")
    };
    assert!(matches!(**bc, Expr::Binary(_, BinOp::Mul, _)));
    assert_eq!(**d, Expr::column("D"));
    let Some(Expr::Or(and, z)) = &sel.where_clause else {
        panic!()
    };
    assert!(matches!(**z, Expr::Compare(..)));
    let Expr::And(not, _) = &**and else { panic!() };
    assert!(matches!(**not, Expr::Not(_)));
    // Joins nest left to right.
    let s = sys("SELECT * FROM a JOIN b ON a.x = b.x LEFT JOIN c ON b.y = c.y CROSS JOIN d");
    let sel = select(&s);
    let TableRef::Join {
        left,
        kind: JoinKind::Cross,
        on: None,
        ..
    } = &sel.from[0]
    else {
        panic!()
    };
    let TableRef::Join {
        left: ab,
        kind: JoinKind::Left,
        on: Some(_),
        ..
    } = &**left
    else {
        panic!()
    };
    assert!(matches!(
        **ab,
        TableRef::Join {
            kind: JoinKind::Inner,
            ..
        }
    ));
    // Parameters number in order; host variables keep indicators.
    let s = sys("SELECT * FROM t WHERE a = ? AND b = :B:BI AND c = ?");
    let cols = columns_of(select(&s).where_clause.as_ref().unwrap());
    assert_eq!(cols.len(), 3);
    let Some(Expr::And(l, r)) = &select(&s).where_clause else {
        panic!()
    };
    assert!(matches!(**r, Expr::Compare(_, _, ref p, _) if **p == Expr::Param(1)));
    let Expr::And(first, host) = &**l else {
        panic!()
    };
    assert!(matches!(**first, Expr::Compare(_, _, ref p, _) if **p == Expr::Param(0)));
    assert!(
        matches!(**host, Expr::Compare(_, _, ref h, _) if matches!(**h, Expr::Host { ref name, ind: Some(ref i) } if name == "B" && i == "BI"))
    );
    // Registers, durations, literals.
    let s = sys("SELECT CURRENT DATE + 3 DAYS, DATE '2026-01-01' FROM d");
    let sel = select(&s);
    let SelectItem::Expr(Expr::Binary(reg, BinOp::Add, dur), _) = &sel.items[0] else {
        panic!()
    };
    assert_eq!(**reg, Expr::Register(Register::Date));
    assert!(matches!(
        **dur,
        Expr::Duration(_, sql::ast::DurationUnit::Days)
    ));
    assert!(
        matches!(&sel.items[1], SelectItem::Expr(Expr::Literal(Literal::Date(d)), _) if d == "2026-01-01")
    );
    // Isolation and FOR UPDATE.
    let Statement::Select(q) = sys("SELECT * FROM t FOR UPDATE OF a, b WITH RR") else {
        panic!()
    };
    assert_eq!(q.for_update.as_ref().unwrap().len(), 2);
    assert_eq!(q.isolation, Some(Isolation::Rr));
    // COUNT(*) and DISTINCT.
    let s = sys("SELECT COUNT(*), COUNT(DISTINCT x) FROM t");
    let sel = select(&s);
    assert!(
        matches!(&sel.items[0], SelectItem::Expr(Expr::Function(f), _) if f.star && f.name == "COUNT")
    );
    assert!(
        matches!(&sel.items[1], SelectItem::Expr(Expr::Function(f), _) if f.distinct && f.args.len() == 1)
    );
}

#[test]
fn ddl_and_dml_shapes() {
    let Statement::CreateTable(t) = sys(
        "CREATE TABLE qstest/cust (cusno DECIMAL(7, 0) NOT NULL PRIMARY KEY, name CHAR(30) NOT NULL WITH DEFAULT, bal NUMERIC(9, 2) DEFAULT 0, note VARCHAR(100) CCSID 37, id INTEGER GENERATED ALWAYS AS IDENTITY (START WITH 100, INCREMENT BY 10), CONSTRAINT uq UNIQUE (name)) RCDFMT cusrec",
    ) else {
        panic!()
    };
    assert_eq!(t.columns.len(), 5);
    assert_eq!(t.columns[0].type_spec.data_type, DataType::Decimal(7, 0));
    assert!(t.columns[0].not_null && t.columns[0].primary_key);
    assert_eq!(t.columns[1].default, Some(ColumnDefault::System));
    assert_eq!(t.columns[2].type_spec.data_type, DataType::Numeric(9, 2));
    assert_eq!(
        t.columns[2].default,
        Some(ColumnDefault::Value(Expr::num("0")))
    );
    assert_eq!(t.columns[3].type_spec.ccsid, Some(37));
    let id = t.columns[4].identity.as_ref().unwrap();
    assert_eq!((id.always, id.start, id.increment), (true, 100, 10));
    assert!(
        matches!(&t.constraints[0], Constraint::Unique { name: Some(n), columns } if n.value == "UQ" && columns.len() == 1)
    );
    assert_eq!(t.rcdfmt.as_ref().unwrap().value, "CUSREC");
    let Statement::AlterTable(a) = sys(
        "ALTER TABLE cust ADD COLUMN phone CHAR(15), DROP COLUMN note, ALTER COLUMN bal DROP DEFAULT, DROP PRIMARY KEY",
    ) else {
        panic!()
    };
    assert_eq!(a.actions.len(), 4);
    assert!(matches!(&a.actions[0], AlterAction::AddColumn(c) if c.name.value == "PHONE"));
    assert!(matches!(&a.actions[2], AlterAction::SetDefault(_, None)));
    assert_eq!(a.actions[3], AlterAction::DropPrimaryKey);
    let Statement::CreateIndex(i) = sys("CREATE UNIQUE INDEX ix ON cust (name DESC, cusno)") else {
        panic!()
    };
    assert!(i.unique && !i.where_not_null);
    assert_eq!(
        i.columns,
        vec![
            (sql::ast::Ident::new("NAME"), true),
            (sql::ast::Ident::new("CUSNO"), false)
        ]
    );
    let Statement::Insert(ins) =
        sys("INSERT INTO cust (cusno, name) VALUES (1, 'Alice'), (2, 'Bob') WITH NC")
    else {
        panic!()
    };
    assert_eq!(ins.columns.len(), 2);
    assert!(
        matches!(&ins.source, InsertSource::Values(rows) if rows.len() == 2 && rows[1][1] == Expr::str("Bob"))
    );
    assert_eq!(ins.isolation, Some(Isolation::Nc));
    let Statement::Update(u) =
        sys("UPDATE cust SET (bal, note) = (0, DEFAULT), name = 'x' WHERE CURRENT OF c1")
    else {
        panic!()
    };
    assert_eq!(u.set.len(), 2);
    assert_eq!(u.set[0].columns.len(), 2);
    assert_eq!(u.set[0].values[1], Expr::Default);
    assert!(matches!(&u.where_clause, Where::CurrentOf(c) if c.value == "C1"));
    let Statement::Delete(d) = sys("DELETE FROM cust") else {
        panic!()
    };
    assert_eq!(d.where_clause, Where::All);
    let Statement::Merge(m) = sys(
        "MERGE INTO cust c USING newcust n ON c.cusno = n.cusno WHEN MATCHED THEN UPDATE SET bal = n.bal WHEN NOT MATCHED THEN INSERT VALUES (n.cusno, n.name)",
    ) else {
        panic!()
    };
    assert_eq!(m.actions.len(), 2);
    assert!(
        matches!(&m.actions[1], MergeAction::Insert { columns, values, .. } if columns.is_empty() && values.len() == 2)
    );
    let Statement::Fetch(f) = sys("FETCH RELATIVE -2 FROM c2 INTO :X") else {
        panic!()
    };
    assert!(matches!(
        f.orientation,
        FetchOrientation::Relative(Expr::Neg(_))
    ));
    assert_eq!(f.into.len(), 1);
    let Statement::SetSchema(SchemaTarget::Name(n)) = sys("SET SCHEMA = 'MyLib'") else {
        panic!()
    };
    assert_eq!(n.value, "MyLib");
    let Statement::LabelOn(l) = sys("LABEL ON TABLE cust (cusno IS 'Number', name TEXT IS 'Name')")
    else {
        panic!()
    };
    assert_eq!(l.columns.len(), 2);
    assert!(!l.columns[0].2 && l.columns[1].2);
}

#[test]
fn naming_modes() {
    // *SYS: slash qualifies; a dot after a table is a column.
    let s = sys("SELECT c.name FROM qstest/cust c");
    let sel = select(&s);
    assert!(
        matches!(&sel.items[0], SelectItem::Expr(Expr::Column(c), _) if c.table.as_ref().unwrap().value == "C")
    );
    assert!(matches!(
        parse("SELECT * FROM qstest.cust", Naming::Sys),
        Err(SqlError::Syntax { .. })
    ));
    // *SQL: dot qualifies.
    let s = sqln("SELECT * FROM qstest.cust");
    assert!(
        matches!(&select(&s).from[0], TableRef::Table { name, .. } if name.schema.as_ref().unwrap().value == "QSTEST")
    );
    assert!(matches!(
        parse("SELECT * FROM qstest/cust", Naming::Sql),
        Err(SqlError::Syntax { .. })
    ));
    // Three parts are refused either way.
    let e = parse("SELECT * FROM a.b.c", Naming::Sql).unwrap_err();
    assert!(matches!(e, SqlError::ThreePartName { .. }));
    assert_eq!(
        (e.sqlcode(), e.sqlstate(), e.msgid().as_str()),
        (-104, "42601", "SQL0104")
    );
    // Delimited identifiers keep case and are never keywords.
    let s = sqln("SELECT \"from\", \"MiXed\" FROM \"my table\"");
    let sel = select(&s);
    assert!(
        matches!(&sel.items[0], SelectItem::Expr(Expr::Column(c), _) if c.column.value == "from" && c.column.delimited)
    );
    assert!(matches!(&sel.from[0], TableRef::Table { name, .. } if name.name.value == "my table"));
    // LABEL ON COLUMN in each mode.
    assert!(matches!(
        sys("LABEL ON COLUMN qstest/cust.name IS 'x'"),
        Statement::LabelOn(_)
    ));
    assert!(matches!(
        sqln("LABEL ON COLUMN qstest.cust.name IS 'x'"),
        Statement::LabelOn(_)
    ));
}

#[test]
fn errors() {
    let e = parse("SELECT FROM", Naming::Sys).unwrap_err();
    let SqlError::Syntax {
        line, col, found, ..
    } = &e
    else {
        panic!("{e}")
    };
    assert_eq!((*line, *col, found.as_str()), (1, 8, "FROM"));
    assert_eq!(e.sqlcode(), -104);
    let e = parse("SELECT * FROM t WHERE a = 1 extra", Naming::Sys).unwrap_err();
    assert!(
        matches!(&e, SqlError::Syntax { found, expected, .. } if found == "EXTRA" && expected == "end of statement")
    );
    let e = parse("SELECT *\nFROM t\nWHERE a =", Naming::Sys).unwrap_err();
    assert!(
        matches!(
            &e,
            SqlError::Syntax {
                line: 3,
                col: 10,
                ..
            }
        ),
        "{e}"
    );
    for (text, what) in [
        (
            "WITH x AS (SELECT 1 FROM d) SELECT * FROM x",
            "common table",
        ),
        ("SELECT 1 FROM a EXCEPT SELECT 1 FROM b", "EXCEPT"),
        ("CREATE TABLE t (c CLOB(1K))", "CLOB"),
        ("CALL p()", "CALL"),
        ("SET TRANSACTION ISOLATION LEVEL RR", "SET TRANSACTION"),
        ("DECLARE c CURSOR FOR s1", "prepared statement"),
    ] {
        let e = parse(text, Naming::Sys).unwrap_err();
        assert!(
            matches!(&e, SqlError::Unsupported { what: w } if w.contains(what)),
            "{text}: {e}"
        );
        assert_eq!(e.sqlcode(), -84);
    }
    assert!(matches!(
        parse("SELECT X'ABC' FROM d", Naming::Sys),
        Err(SqlError::Literal { .. })
    ));
    assert!(matches!(
        parse("CREATE TABLE t (c DECIMAL(70))", Naming::Sys),
        Err(SqlError::Syntax { .. })
    ));
    assert!(matches!(
        parse("UPDATE t SET (a, b) = (1, 2, 3)", Naming::Sys),
        Err(SqlError::Syntax { .. })
    ));
}
