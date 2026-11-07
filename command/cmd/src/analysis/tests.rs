// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use compdiag::front::SymbolKind;

use super::*;

const SRC: &str = "\
             CMD        PROMPT('Copy it')
             PARM       KWD(FROM) TYPE(Q1) MIN(1) PROMPT('From')
             PARM       KWD(TO) TYPE(*NAME) LEN(10) +
                          PMTCTL(SHOW) PROMPT('To')
 Q1:         QUAL       TYPE(*NAME) LEN(10)
             QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL)
 SHOW:       PMTCTL     CTL(FROM) COND((*NE ' '))
             DEP        CTL(&TO) PARM((&FROM *NE &TO))
";

fn text(a: &Analysis, s: Span) -> &str {
    &a.sources[0].text[s.start..s.end]
}

#[test]
fn keywords_lists_and_their_uses() {
    let a = analyze(
        &Source::member("QGPL", "QCMDSRC", "CPYIT", SRC),
        &Options::default(),
    );
    assert!(a.diagnostics.is_empty(), "{:?}", a.diagnostics);
    assert!(a.ast.definition.is_some());
    let decls: Vec<(&str, SymbolKind)> = a
        .declarations
        .iter()
        .map(|d| (text(&a, d.span), d.kind))
        .collect();
    assert_eq!(
        decls,
        [
            ("CMD", SymbolKind::Command),
            ("FROM", SymbolKind::Keyword),
            ("TO", SymbolKind::Keyword),
            ("Q1", SymbolKind::Label),
            ("SHOW", SymbolKind::Label),
        ]
    );
    assert_eq!(a.declarations[0].name, "CPYIT");
    let refs: Vec<(&str, u32, Option<&str>)> = a
        .references
        .iter()
        .map(|r| {
            (
                text(&a, r.span),
                r.span.line,
                r.declaration.map(|i| a.declarations[i].name.as_str()),
            )
        })
        .collect();
    assert_eq!(
        refs,
        [
            ("Q1", 2, Some("Q1")),
            ("SHOW", 4, Some("SHOW")),
            ("FROM", 7, Some("FROM")),
            ("TO", 8, Some("TO")),
            ("FROM", 8, Some("FROM")),
            ("TO", 8, Some("TO")),
        ]
    );
}

#[test]
fn errors_carry_their_line() {
    let src = "             CMD        PROMPT('X')
             PARM       KWD(A) TYPE(*CHAR) LEN(1)
             PARM       KWD(A) TYPE(*CHAR) LEN(1)
";
    let a = analyze(
        &Source::text(src),
        &Options {
            name: Some("X".into()),
        },
    );
    assert!(a.ast.definition.is_none());
    let d = &a.diagnostics[0];
    assert_eq!(d.line, 3, "{d:?}");
    assert_eq!(d.column, 14);
    assert!(d.id.starts_with("CPD"));
    assert!(!d.text.is_empty());
}
