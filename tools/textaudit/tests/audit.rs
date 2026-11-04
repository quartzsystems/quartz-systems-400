// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! No new text on display paths: every literal the audit finds is in
//! the baseline, so the count never grows.

use std::path::Path;

fn root() -> std::path::PathBuf {
    textaudit::find_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("the tree")
}

#[test]
fn no_new_literals() {
    let report = textaudit::audit(&root()).expect("the audit runs");
    assert!(!report.fails(), "\n{}", report.details(60));
    assert!(report.total() <= report.baseline);
}

#[test]
fn the_scanner_sees_the_canonical_failure() {
    let src = r#"
        fn f() -> Fail { Fail::new("CPD0084", format!("{ty} is not a type of resource.")) }
        fn g() { info!("log line, not text"); x.expect("never shown"); }
        #[cfg(test)]
        mod tests { fn t() { let _ = "Test only text"; } }
        fn h() -> &'static str { "Operational" }
    "#;
    let lits = textaudit::scan::literals(src).expect("tokens");
    let text: Vec<&str> = lits
        .iter()
        .filter(|l| textaudit::prose::is_prose(&l.text))
        .map(|l| l.text.as_str())
        .collect();
    assert_eq!(text, ["{ty} is not a type of resource.", "Operational"]);
    let k = textaudit::kind_of(&lits[1].text, &lits[1].calls);
    assert_eq!(k, textaudit::Kind::Message);
}
