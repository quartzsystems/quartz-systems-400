// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Print (handoff step 15): the pages against their goldens, the
//! viewer against Navigator's, and what the actions do over the
//! fixture. Golden corrections are listed in docs/ACCESSWEB.md.

mod common;

use accessweb::env::policy::{PolicyStore, Setting};
use accessweb::env::print::Print;
use accessweb::policy;
use common::{Browser, HEADER_USER, assert_golden, golden, proto, relative};
use qsweb::golden::{Normalize, diff};

const INVRPT: &str = "123402-CWELLMAN-INVBATCH-INVRPT-402";

fn fix(g: &mut String, from: &str, to: &str) {
    assert!(g.contains(from), "golden fix not applicable: {from}");
    *g = g.replace(from, to);
}

/// The content cell of a page: crumb to the footer.
fn content(html: &str) -> &str {
    let a = html.find("<div class=\"crumb\">").unwrap();
    let b = html.find("<tr><td class=\"footer\"").unwrap();
    &html[a..b]
}

fn assert_text(served: &str, g: &str, file: &str) {
    let dir = file.split('/').next().unwrap_or("");
    let map = |u: &str| proto(u).map(|t| relative(&t, dir));
    if let Some(d) = diff(served, g, &Normalize { url: &map }) {
        panic!("{file} differs from the served page at {d}");
    }
}

#[test]
fn printer_output_page() {
    let mut b = Browser::signed_on();
    let r = b.get("/print");
    assert_eq!(r.status, 200);
    let mut g = golden("web/printer-output.html").replace("\r\n", "\n");
    fix(&mut g, HEADER_USER.0, HEADER_USER.1);
    // A checkbox says which file it selects.
    let ids = [
        INVRPT,
        "123402-CWELLMAN-INVBATCH-QPJOBLOG-403",
        "123311-CWELLMAN-QPADEV0003-QPRINT-1",
        "123311-CWELLMAN-QPADEV0003-QSYSPRT-2",
        "123290-CWELLMAN-INVBATCH-INVLABEL-1",
        "123101-CWELLMAN-QPADEV0003-QPDSPJOB-1",
        "123050-CWELLMAN-QPADEV0003-QPJOBLOG-1",
        "122990-CWELLMAN-INVBATCH-ASSETEXP-1",
    ];
    for (i, id) in ids.iter().enumerate() {
        fix(
            &mut g,
            &format!("name=\"s{i}\">"),
            &format!("name=\"s{i}\" value=\"{id}\">"),
        );
    }
    // Data: Auto-refresh is off until chosen (the page then refreshes).
    fix(
        &mut g,
        "<option selected>30</option>",
        "<option>30</option>",
    );
    // Nameless buttons in one form cannot be told apart.
    for (v, n) in [
        ("Hold", "hold"),
        ("Release", "release"),
        ("Delete...", "delete"),
        ("Move", "move"),
        ("Save as PDF", "pdf"),
        ("Save as text", "text"),
    ] {
        fix(
            &mut g,
            &format!("<input type=\"submit\" value=\"{v}\">"),
            &format!("<input type=\"submit\" name=\"{n}\" value=\"{v}\">"),
        );
    }
    assert_text(&r.text(), &g, "web/printer-output.html");
}

#[test]
fn splf_props_page() {
    let mut b = Browser::signed_on();
    let r = b.get(&format!("/print/props/{INVRPT}"));
    assert_eq!(r.status, 200);
    assert_golden(&r.text(), "web/splf-props.html", &[HEADER_USER]);
}

#[test]
fn printers_page() {
    let mut b = Browser::signed_on();
    let r = b.get("/print/printers");
    assert_eq!(r.status, 200);
    assert_golden(&r.text(), "web/printers.html", &[HEADER_USER]);
}

/// The viewer is Navigator's page in this product's frame: the content
/// is the golden's but for where it sits and what its links do.
#[test]
fn viewer_is_navigators_page() {
    let mut b = Browser::signed_on();
    let r = b.get(&format!("/print/view/{INVRPT}"));
    assert_eq!(r.status, 200);
    let mut g = content(&golden("nav/spooled-view.html").replace("\r\n", "\n")).to_string();
    // In this product's tree.
    fix(
        &mut g,
        "Navigator &gt; Work management &gt; Output queues &gt; QPRINT &gt; INVRPT",
        "Access for Web &gt; Print &gt; Printer output &gt; INVRPT",
    );
    fix(
        &mut g,
        "<a href=\"spooled-files.html\">Back to QPRINT</a>",
        "<a href=\"../web/printer-output.html\">Back to Printer output</a>",
    );
    // Page 1 of 12: no previous page; the next is page 2.
    fix(
        &mut g,
        "<a href=\"#\">&lt; Previous</a>",
        "<span class=\"grey\">&lt; Previous</span>",
    );
    fix(
        &mut g,
        "<a href=\"#\">Next &gt;</a>",
        "<a href=\"spooled-view.html\">Next &gt;</a>",
    );
    fix(
        &mut g,
        "<a href=\"#\">View as PDF</a> &nbsp; <a href=\"#\">Download text</a>",
        "<a href=\"spooled-view.html\">View as PDF</a> &nbsp; <a href=\"spooled-view.html\">Download text</a>",
    );
    // Hold and Release are links (a GET form cannot hold); Delete asks.
    fix(
        &mut g,
        "<input type=\"submit\" value=\"Hold\"> <input type=\"submit\" value=\"Release\"> <input type=\"submit\" value=\"Delete\">",
        "<a href=\"../web/printer-output.html\">Hold</a> &nbsp; <a href=\"../web/printer-output.html\">Release</a> &nbsp; <a href=\"../shared/confirm.html\">Delete</a>",
    );
    let served = r.text();
    assert_text(content(&served), &g, "nav/spooled-view.html");
    // Find moves to the page holding the text; PDF and text are whole.
    assert!(
        b.get(&format!("/print/view/{INVRPT}?f=OF%20PAGE%20%20%203"))
            .text()
            .contains("value=\"3\"")
    );
    let r = b.get(&format!("/print/view/{INVRPT}/pdf"));
    assert!(r.bytes().starts_with(b"%PDF-"));
    assert!(
        r.header("Content-Disposition")
            .unwrap()
            .starts_with("inline")
    );
    let r = b.get(&format!("/print/view/{INVRPT}/text?save=1"));
    assert!(
        r.header("Content-Disposition")
            .unwrap()
            .starts_with("attachment")
    );
    assert_eq!(r.text().matches('\u{c}').count(), 11);
}

#[test]
fn hold_release_move_delete() {
    let mut b = Browser::signed_on();
    // Hold one at the end of the page.
    let r = b.post(
        "/print",
        &[("s0", INVRPT), ("hold", "Hold"), ("hw", "page")],
    );
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/print"));
    assert!(
        b.get("/print")
            .text()
            .contains("QAD6002</a> - Spooled file INVRPT held.")
    );
    assert!(
        b.fx.print
            .ran
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .ends_with("OPTION(*PAGEEND)")
    );
    // Release two, one of them ready: the summary and each file's result.
    let qsys = "123311-CWELLMAN-QPADEV0003-QSYSPRT-2";
    b.post(
        "/print",
        &[("s0", INVRPT), ("s3", qsys), ("release", "Release")],
    );
    let t = b.get("/print").text();
    assert!(
        t.contains("QAD6006</a> - 1 of 2 actions failed; see the details."),
        "{t}"
    );
    assert!(t.contains("CPF3337"), "{t}");
    assert!(!b.get("/print").text().contains("CPF3337"));
    // Move to the top of another queue.
    b.post(
        "/print",
        &[
            ("s3", qsys),
            ("to", "QGPL/PRT02"),
            ("top", "on"),
            ("move", "Move"),
        ],
    );
    assert!(
        b.get("/print")
            .text()
            .contains("QAD6005</a> - Spooled file QSYSPRT moved to QGPL/PRT02.")
    );
    assert_eq!(b.fx.print.files.lock().unwrap()[0].outq, "QGPL/PRT02");
    // Move to a queue not offered.
    b.post(
        "/print",
        &[("s3", qsys), ("to", "QSYS/NOPE"), ("move", "Move")],
    );
    assert!(b.get("/print").text().contains("XWA0004"));
    // Delete asks first.
    let r = b.post("/print", &[("s1", INVRPT), ("delete", "Delete...")]);
    let to = r.header("Location").unwrap().to_string();
    assert!(to.starts_with("/qs400/accessweb/print/confirm?s="), "{to}");
    assert!(b.get(&to).text().contains("Confirm Delete"));
    let r = b.post(&to, &[("ok", "OK")]);
    assert_eq!(r.status, 303);
    assert!(b.fx.print.find("CWELLMAN", INVRPT).is_err());
    // Save as PDF needs one file.
    b.post(
        "/print",
        &[
            ("s0", qsys),
            ("s1", "123290-CWELLMAN-INVBATCH-INVLABEL-1"),
            ("pdf", "Save as PDF"),
        ],
    );
    assert!(b.get("/print").text().contains("XWA0045"));
    let r = b.post("/print", &[("s0", qsys), ("text", "Save as text")]);
    assert_eq!(
        r.header("Location"),
        Some(format!("/qs400/accessweb/print/view/{qsys}/text?save=1").as_str())
    );
}

#[test]
fn filters_paging_and_auto_refresh() {
    let mut b = Browser::signed_on();
    let t = b.get("/print?s=HLD").text();
    assert!(t.contains("1 entry") && t.contains(">QPRINT</a>"), "{t}");
    let t = b.get("/print?u=*ALL").text();
    assert!(t.contains("QPGMR") && t.contains("9 entries"), "{t}");
    let t = b.get("/print?rows=10&qn=QEZJOBLOG").text();
    assert!(t.contains("2 entries"), "{t}");
    let t = b.get("/print?auto=30").text();
    assert!(
        t.contains("<meta http-equiv=\"refresh\" content=\"30\" />")
            && t.contains("<option selected>30</option>"),
        "{t}"
    );
    // The page shown sorted by pages.
    let t = b.get("/print?sort=p").text();
    assert!(t.find(">QPJOBLOG</a>").unwrap() < t.find(">INVLABEL</a>").unwrap());
}

#[test]
fn policies_and_authority() {
    // WEBUSER has no *SPLCTL: other users' output is the system's to refuse.
    let mut w = Browser::as_user("WEBUSER");
    let t = w.get("/print?u=*ALL").text();
    assert!(!t.contains("QPGMR"), "{t}");
    assert!(
        w.get(&format!("/print/props/{INVRPT}"))
            .text()
            .contains("CPF3303")
            || w.get("/print").status == 200
    );
    // Policies take actions away and refuse their posts.
    let mut b = Browser::signed_on();
    b.fx.policy
        .put(
            "ADMINS",
            &[
                (policy::PRINT_OWN.into(), Setting::Deny),
                (policy::PRINT_PDF.into(), Setting::Deny),
            ],
        )
        .unwrap();
    let t = b.get("/print").text();
    assert!(
        !t.contains("name=\"hold\"") && !t.contains("Save as PDF") && !t.contains("/pdf\""),
        "{t}"
    );
    let r = b.post("/print", &[("s0", INVRPT), ("hold", "Hold")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("XWA0001"));
    assert_eq!(b.get(&format!("/print/view/{INVRPT}/pdf")).status, 403);
    b.fx.policy
        .put("ADMINS", &[(policy::PRINT_OTHERS.into(), Setting::Deny)])
        .unwrap();
    assert_eq!(b.get("/print?u=*ALL").status, 403);
    // A file still being written.
    b.fx.policy.put("ADMINS", &[]).unwrap();
    b.fx.print.files.lock().unwrap()[0].status = "OPN".into();
    b.get(&format!("/print/view/{INVRPT}"));
    assert!(b.get("/print").text().contains("QAD6012"));
}
