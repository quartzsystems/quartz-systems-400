// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use compdiag::front::SymbolKind;

use super::*;

fn text(a: &Analysis, s: Span) -> &str {
    &a.sources[0].text[s.start..s.end]
}

#[test]
fn a_script_declares_and_uses() {
    let src = "CREATE TABLE QGPL.ORDERS (ID INT NOT NULL, \"Total\" DECIMAL(9, 2),
  PRIMARY KEY (ID));
CREATE INDEX QGPL.ORDX ON QGPL.ORDERS (ID);
SELECT ID, \"Total\", EXTRACT(YEAR FROM CURRENT DATE)
  FROM QGPL.ORDERS O JOIN QGPL.CUST C ON O.ID = C.ID
  WHERE ID > 10;
CALL QGPL.REPORT(1);
";
    let a = analyze(&Source::text(src), &Options::default());
    assert!(a.diagnostics.is_empty(), "{:?}", a.diagnostics);
    assert_eq!(a.ast.statements.len(), 4);
    assert!(a.ast.statements.iter().all(|s| s.statement.is_some()));
    assert_eq!(
        text(&a, a.ast.statements[1].span),
        "CREATE INDEX QGPL.ORDX ON QGPL.ORDERS (ID)"
    );
    let decls: Vec<(&str, SymbolKind)> = a
        .declarations
        .iter()
        .map(|d| (text(&a, d.span), d.kind))
        .collect();
    assert_eq!(
        decls,
        [
            ("ORDERS", SymbolKind::Table),
            ("ID", SymbolKind::Field),
            ("\"Total\"", SymbolKind::Field),
            ("ORDX", SymbolKind::Index),
        ]
    );
    let refs: Vec<(&str, u32, SymbolKind, bool)> = a
        .references
        .iter()
        .map(|r| {
            (
                text(&a, r.span),
                r.span.line,
                r.kind,
                r.declaration.is_some(),
            )
        })
        .collect();
    assert_eq!(
        refs,
        [
            ("ID", 2, SymbolKind::Field, true),
            ("ORDERS", 3, SymbolKind::Table, true),
            ("ID", 3, SymbolKind::Field, true),
            ("ID", 4, SymbolKind::Field, true),
            ("\"Total\"", 4, SymbolKind::Field, true),
            ("ORDERS", 5, SymbolKind::Table, true),
            ("CUST", 5, SymbolKind::Table, false),
            ("ID", 5, SymbolKind::Field, true),
            ("ID", 5, SymbolKind::Field, true),
            ("ID", 6, SymbolKind::Field, true),
            ("REPORT", 7, SymbolKind::Routine, false),
        ]
    );
}

#[test]
fn a_bad_statement_is_reported_and_the_rest_read() {
    let src = "SELECT * FROM T1;
SELECT FROM WHERE;
DECLARE C1 CURSOR FOR SELECT * FROM T2;
";
    let a = analyze(
        &Source::member("QGPL", "QSQLSRC", "S", src),
        &Options::default(),
    );
    assert_eq!(a.ast.statements.len(), 3);
    assert!(a.ast.statements[1].statement.is_none());
    let d = &a.diagnostics[0];
    assert_eq!((d.line, d.severity), (2, 30), "{d:?}");
    assert!(d.id.starts_with("SQL0"), "{d:?}");
    assert!(d.column > 0);
    let c = a.declarations.iter().find(|d| d.name == "C1").unwrap();
    assert_eq!(
        (c.kind, c.span.line, c.span.column),
        (SymbolKind::Cursor, 3, 9)
    );
    assert!(a.references.iter().any(|r| r.name == "T2"));
}
