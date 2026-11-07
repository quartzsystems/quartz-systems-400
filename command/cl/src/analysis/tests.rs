// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use compdiag::front::SymbolKind;

use super::*;
use crate::sem::FieldDef;

fn text(a: &Analysis, s: Span) -> &str {
    &a.sources[0].text[s.start..s.end]
}

const SRC: &str = "\
             PGM        PARM(&NAME)
             DCL        VAR(&NAME) TYPE(*CHAR) LEN(10)
             DCL        VAR(&N) TYPE(*DEC) LEN(5 0)
             DCLF       FILE(MYLIB/CUSTF)
 LOOP:       CHGVAR     VAR(&N) VALUE(&N + 1) /* &NOT a use */
             IF         COND(&N *LT 10) THEN(GOTO CMDLBL(LOOP))
             SNDPGMMSG  MSG('&NAME is' *BCAT &NAME *BCAT &CUSTNO)
             CALLSUBR   SUBR(SHOW)
             SUBR       SUBR(SHOW)
             CHGVAR     &OOPS 1
             ENDSUBR
             ENDPGM
";

#[test]
fn variables_labels_and_files() {
    let fetch = |name: &str, lib: &str| {
        (name == "CUSTF" && lib == "MYLIB").then(|| {
            (
                false,
                vec![Format {
                    name: "CUSTR".into(),
                    fields: vec![FieldDef {
                        name: "CUSTNO".into(),
                        ty: 'P',
                        pos: 1,
                        len: 4,
                        digits: 7,
                        decimals: 0,
                        in_pos: None,
                    }],
                    ..Format::default()
                }],
            )
        })
    };
    let a = analyze(
        &Source::member("QGPL", "QCLSRC", "T", SRC),
        &Options {
            files: Some(&fetch),
        },
    );
    let decls: Vec<(&str, SymbolKind, u32)> = a
        .declarations
        .iter()
        .map(|d| (text(&a, d.span), d.kind, d.span.line))
        .collect();
    assert_eq!(
        decls,
        [
            ("NAME", SymbolKind::Variable, 2),
            ("N", SymbolKind::Variable, 3),
            ("CUSTF", SymbolKind::File, 4),
            ("LOOP", SymbolKind::Label, 5),
            ("SHOW", SymbolKind::Subroutine, 9),
            ("CUSTF", SymbolKind::Field, 4),
        ]
    );
    assert_eq!(a.declarations[5].name, "CUSTNO");
    let uses = |n: &str| a.references.iter().filter(|r| r.name == n).count();
    // PARM(&NAME) and the one outside the quotes; the comment is no use.
    assert_eq!(uses("NAME"), 2);
    assert_eq!(uses("N"), 3);
    assert_eq!(uses("NOT"), 0);
    let custno = a.references.iter().find(|r| r.name == "CUSTNO").unwrap();
    assert_eq!(
        a.declarations[custno.declaration.unwrap()].kind,
        SymbolKind::Field
    );
    let goto = a.references.iter().find(|r| r.name == "LOOP").unwrap();
    assert_eq!((goto.span.line, text(&a, goto.span)), (6, "LOOP"));
    assert_eq!(goto.declaration, Some(3));
    let call = a.references.iter().find(|r| r.name == "SHOW").unwrap();
    assert_eq!((call.span.line, call.declaration), (8, Some(4)));
    // &OOPS is not declared: the generator's message, at its line.
    let d: Vec<(&str, u32)> = a
        .diagnostics
        .iter()
        .map(|d| (d.id.as_str(), d.line))
        .collect();
    assert_eq!(d, [("CPD0727", 10)]);
    assert_eq!(a.diagnostics[0].column, 14);
    assert!(!a.diagnostics[0].help.is_empty());
}

#[test]
fn a_missing_file_and_label() {
    let src = "PGM
DCLF FILE(NOFILE)
GOTO NOWHERE
ENDPGM
";
    let a = analyze(&Source::text(src), &Options::default());
    let d: Vec<(&str, u32)> = a
        .diagnostics
        .iter()
        .map(|d| (d.id.as_str(), d.line))
        .collect();
    assert_eq!(d, [("CPD0744", 2), ("CPD0735", 3)]);
}
