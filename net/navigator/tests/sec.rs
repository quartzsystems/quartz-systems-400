// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Security: the audit journal against its golden page. Golden
//! corrections are listed in docs/NAVIGATOR.md.

mod common;

use common::{Browser, assert_golden};
use navigator::env::Audit;
use navigator::routes::sec::line;

fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

#[test]
fn audit_journal() {
    let mut b = Browser::signed_on();
    // The page showed 14 of its 25 rows; the next eleven entries.
    let more: String = navigator::fixture::sec::entries()[14..25]
        .iter()
        .map(|e| line(&navigator::util::mdy(e.at, 0).1, e) + "\n")
        .collect();
    let tail = "WAREHOUSE *CHANGE";
    let with_more = format!("{tail}\n{}", more.trim_end());
    assert_golden(
        &ok(&mut b, "/audit"),
        "nav/audit-journal.html",
        &[
            (tail, with_more.as_str()),
            (
                "<a href=\"#\">Download as CSV</a>",
                "<a href=\"audit-journal.html\">Download as CSV</a>",
            ),
        ],
    );
}

#[test]
fn filters_and_csv() {
    let mut b = Browser::signed_on();
    // The home page's failed sign-ons link.
    let t = ok(&mut b, "/audit?t=PW");
    assert!(
        t.contains("<option selected>PW - Password</option>") && t.contains("5 entries"),
        "{t}"
    );
    assert!(!t.contains("  AF  "));
    let t = ok(&mut b, "/audit?t=PW%20-%20Password&u=guest");
    assert!(t.contains("3 entries"));
    let t = ok(&mut b, "/audit?f=10/28/26&to=10/28/26");
    assert!(t.contains("0 entries"));
    let r = b.get("/audit?t=AF&csv=1");
    assert_eq!(r.text().lines().count(), 4);
    assert!(r.text().starts_with("Date,Time,Type,User,Job,Detail"));
}

#[test]
fn needs_audit_authority() {
    let mut b = Browser::as_user("QSYSOPR");
    let r = b.get("/audit");
    assert_eq!(r.status, 403);
    assert!(r.text().contains("*AUDIT"));
    assert!(
        b.fx.system
            .audit
            .lock()
            .unwrap()
            .iter()
            .any(|a| matches!(a, Audit::Refused { need, .. } if need == "*AUDIT"))
    );
}
