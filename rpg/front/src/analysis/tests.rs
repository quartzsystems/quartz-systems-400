// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use compdiag::front::SymbolKind;

use super::*;

/// The text a span covers.
fn text(a: &Analysis, source: usize, span: Span) -> &str {
    &a.sources[source].text[span.start..span.end]
}

#[test]
fn fixed_form_spans_land_on_the_names() {
    let src = "     FCUSTOMER  IF   E           K DISK
     D Total           S             11P 2 INZ(0)
     D Rec             DS
     D  Id                     1      5S 0
     C     KEY           CHAIN     CUSTOMER
     C                   EVAL      Total = Total + Id
     C                   EXSR      AddUp
     C     AddUp         BEGSR
     C                   ENDSR
";
    let a = analyze(
        &Source::member("QGPL", "QRPGLESRC", "T", src),
        &Options::default(),
    );
    assert!(a.diagnostics.is_empty(), "{:?}", a.diagnostics);
    let names: Vec<(&str, SymbolKind)> = a
        .declarations
        .iter()
        .map(|d| (text(&a, d.source, d.span), d.kind))
        .collect();
    assert_eq!(
        names,
        [
            ("CUSTOMER", SymbolKind::File),
            ("Total", SymbolKind::Variable),
            ("Rec", SymbolKind::Structure),
            ("Id", SymbolKind::Subfield),
            ("AddUp", SymbolKind::Subroutine),
        ]
    );
    let total = &a.declarations[1];
    assert_eq!((total.span.line, total.span.column), (2, 8));
    // Every reference to Total, Id, CUSTOMER and AddUp finds its declaration.
    for r in &a.references {
        assert_eq!(text(&a, r.source, r.span).to_ascii_uppercase(), r.name);
        if r.name != "KEY" {
            assert!(r.declaration.is_some(), "{r:?}");
        }
    }
    let totals = a.references.iter().filter(|r| r.name == "TOTAL").count();
    assert_eq!(totals, 2);
    let exsr = a.references.iter().find(|r| r.name == "ADDUP").unwrap();
    assert_eq!(exsr.kind, SymbolKind::Subroutine);
    assert_eq!((exsr.span.line, exsr.span.column), (7, 36));
}

#[test]
fn free_form_copy_is_fetched_and_mapped() {
    let main = "**FREE
dcl-s count int(10);
/copy QPROTOS,UTIL
count = bump(count);
dcl-proc local;
  dcl-s count packed(5);
  count += 1;
end-proc;
";
    let util = "**FREE
dcl-pr bump int(10);
  n int(10) value;
end-pr;
";
    let asked = std::cell::RefCell::new(Vec::new());
    let fetch = |r: &IncludeRequest| {
        asked.borrow_mut().push(r.clone());
        (r.member == "UTIL").then(|| Source::member("QGPL", "QPROTOS", "UTIL", util))
    };
    let opts = Options {
        fetch: Some(&fetch),
        ..Options::default()
    };
    let a = analyze(&Source::member("QGPL", "QRPGLESRC", "MAIN", main), &opts);
    assert!(a.diagnostics.is_empty(), "{:?}", a.diagnostics);
    assert_eq!(asked.borrow()[0].file, "QPROTOS");
    assert_eq!(a.sources.len(), 2);
    assert_eq!(a.includes.len(), 1);
    assert_eq!(a.includes[0].resolved, Some(1));
    assert_eq!(a.includes[0].span.line, 3);
    let bump = a.declarations.iter().find(|d| d.name == "BUMP").unwrap();
    assert_eq!((bump.source, bump.span.line, bump.span.column), (1, 2, 8));
    let call = a.references.iter().find(|r| r.name == "BUMP").unwrap();
    assert_eq!((call.source, call.span.line, call.span.column), (0, 4, 9));
    assert_eq!(
        call.declaration.map(|i| a.declarations[i].name.as_str()),
        Some("BUMP")
    );
    // The local `count` in the procedure links to its own declaration.
    let local = a
        .references
        .iter()
        .find(|r| r.name == "COUNT" && r.span.line == 7)
        .unwrap();
    let d = &a.declarations[local.declaration.unwrap()];
    assert_eq!((d.span.line, d.scope.as_deref()), (6, Some("LOCAL")));
    let outer = a
        .references
        .iter()
        .find(|r| r.name == "COUNT" && r.span.line == 4)
        .unwrap();
    assert_eq!(a.declarations[outer.declaration.unwrap()].span.line, 2);
}

#[test]
fn diagnostics_point_into_the_copied_member() {
    let main = "**FREE
/copy QPROTOS,BAD
/copy QPROTOS,MISSING
";
    let bad = "**FREE
dcl-s x int(10)
";
    let fetch = |r: &IncludeRequest| {
        (r.member == "BAD").then(|| Source::member("QGPL", "QPROTOS", "BAD", bad))
    };
    let opts = Options {
        fetch: Some(&fetch),
        ..Options::default()
    };
    let a = analyze(&Source::member("QGPL", "QRPGLESRC", "MAIN", main), &opts);
    let missing = a.diagnostics.iter().find(|d| d.id == "RNF0273").unwrap();
    assert_eq!(missing.line, 3);
    assert!(missing.text.contains("MISSING"), "{}", missing.text);
    assert!(!missing.help.is_empty());
    let semi = a.diagnostics.iter().find(|d| d.id == "RNF5507").unwrap();
    assert_eq!(semi.line, 2);
    match &semi.at {
        Where::Member(m) => assert_eq!(m.member, "BAD"),
        other => panic!("{other:?}"),
    }
    assert_eq!(a.includes[1].resolved, None);
}
