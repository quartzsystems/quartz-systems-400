// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use compdiag::front::SymbolKind;

use super::*;

fn text(a: &Analysis, s: Span) -> &str {
    &a.sources[0].text[s.start..s.end]
}

#[test]
fn formats_fields_and_keys() {
    let src = [
        "     A                                      UNIQUE",
        "     A          R CUSTR                     TEXT('Customers')",
        "     A            CUSTNO         7P 0       COLHDG('Number')",
        "     A            NAME          30A",
        "     A            CITY      R               REFFLD(NAME *SRC)",
        "     A          K CUSTNO",
    ]
    .join("\n");
    let a = analyze(
        &Source::member("QGPL", "QDDSSRC", "CUST", &src),
        &Options::default(),
    );
    assert!(
        a.diagnostics.iter().all(|d| d.severity < 20),
        "{:?}",
        a.diagnostics
    );
    assert!(matches!(a.ast.definition, Some(Definition::Physical(_))));
    let decls: Vec<(&str, SymbolKind, u32, u32)> = a
        .declarations
        .iter()
        .map(|d| (text(&a, d.span), d.kind, d.span.line, d.span.column))
        .collect();
    assert_eq!(
        decls,
        [
            ("CUSTR", SymbolKind::Format, 2, 19),
            ("CUSTNO", SymbolKind::Field, 3, 19),
            ("NAME", SymbolKind::Field, 4, 19),
            ("CITY", SymbolKind::Field, 5, 19),
        ]
    );
    assert_eq!(a.declarations[1].detail, "7P 0");
    assert_eq!(a.declarations[1].scope.as_deref(), Some("CUSTR"));
    let refs: Vec<(&str, u32, u32, Option<usize>)> = a
        .references
        .iter()
        .map(|r| (text(&a, r.span), r.span.line, r.span.column, r.declaration))
        .collect();
    assert_eq!(refs, [("NAME", 5, 52, Some(2)), ("CUSTNO", 6, 19, Some(1))]);
}

#[test]
fn errors_point_at_their_record() {
    let src = "     A          R CUSTR\n     A            CUSTNO         7X 0\n";
    let a = analyze(&Source::text(src), &Options::default());
    let d = a.diagnostics.iter().find(|d| d.severity >= 20).unwrap();
    assert_eq!(d.line, 2, "{d:?}");
    assert_eq!(d.column, 6);
    assert!(d.id.starts_with("DDS") || d.id.starts_with("CPD"));
    assert!(a.ast.definition.is_none());
}
