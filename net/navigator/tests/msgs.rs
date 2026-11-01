// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages: the pages against the golden pages, and what their actions
//! do. Golden corrections are listed in docs/NAVIGATOR.md.

mod common;

use common::{Browser, assert_golden};

/// A page's text, which must be served with 200.
fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

fn ran(b: &Browser) -> Vec<String> {
    b.fx.msgs.ran.lock().unwrap().clone()
}

/// The key of the message `id` on `QSYS/QSYSOPR`.
fn key_of(b: &Browser, id: &str) -> String {
    let qs = b.fx.msgs.queues.lock().unwrap();
    qs["QSYS/QSYSOPR"].iter().rev().find(|m| m.id == id).unwrap().key.clone()
}

/// `html` with the list table cut after its first `n` rows.
fn first_rows(html: &str, n: usize) -> String {
    let start = html.find("<table class=\"list\"").unwrap();
    let mut at = start;
    for _ in 0..=n {
        at += html[at..].find("</tr>").unwrap() + "</tr>".len();
    }
    let end = at + html[at..].find("</table>").unwrap();
    format!("{}{}", &html[..at], &html[end..])
}

/// Jobs that are no longer in the system are not links.
const ENDED: &[(&str, &str)] = &[
    (
        "<td class=\"mono\"><a href=\"job-props-general.html\">123499/QPGMR/RGZPFM00</a></td>",
        "<td class=\"mono\">123499/QPGMR/RGZPFM00</td>",
    ),
    (
        "<td class=\"mono\"><a href=\"job-props-general.html\">123150/QSPLJOB/PRT01</a></td>",
        "<td class=\"mono\">123150/QSPLJOB/PRT01</td>",
    ),
    (
        "<td class=\"mono\"><a href=\"job-props-general.html\">123380/QTCP/QTCPIP</a></td>",
        "<td class=\"mono\">123380/QTCP/QTCPIP</td>",
    ),
];

#[test]
fn qsysopr() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/msgq/QSYS/QSYSOPR");
    // The prototype repeats its first ten messages to fill the page;
    // the fixture's 41 are distinct, so the first ten are compared.
    let g = first_rows(&common::golden("nav/qsysopr.html"), 10);
    common::assert_golden_text(&first_rows(&t, 10), g, "nav/qsysopr.html", ENDED);
    assert_eq!(t.matches("<td class=\"act\">").count(), 25);
    assert_eq!(t.matches("<td><b>").count(), 3, "three inquiries waiting");
    assert!(t.contains("41 entries"));
    // Page 2 has the rest.
    let t = ok(&mut b, "/msgq/QSYS/QSYSOPR?page=2");
    assert!(t.contains("Page 2 of 2"));
    // Filters.
    let t = ok(&mut b, "/msgq/QSYS/QSYSOPR?t=Inquiry+%28unanswered%29");
    assert!(t.contains("3 entries"), "{t}");
    let t = ok(&mut b, "/msgq/QSYS/QSYSOPR?d=10%2F30%2F26");
    assert!(t.contains("No messages on the queue."));
    let t = ok(&mut b, "/msgq/QSYS/QSYSOPR?sort=sev");
    assert!(t.find("CPA5305").unwrap() < t.find("CPF0907").unwrap());
    // Another queue through the select.
    let r = b.get("/msgq/QSYS/QSYSOPR?q=QSYSMSG&t=All");
    assert_eq!(r.header("Location"), Some("/qs400/navigator/msgq/QSYS/QSYSMSG?t=All"));
    assert!(ok(&mut b, "/msgq/QSYS/QSYSMSG").contains("<option selected>QSYSMSG</option>"));
}

#[test]
fn my_messages() {
    let mut b = Browser::signed_on();
    b.get("/nav?close=Messages&back=/qs400/navigator/home");
    let t = ok(&mut b, "/mymsgs");
    assert_golden(&t, "nav/my-messages.html", &[]);
    // Send: SNDMSG to the user, an inquiry's reply to my queue.
    let r = b.post("/mymsgs", &[("to", "CWELLMAN"), ("toq", ""), ("m", "Lunch at 12?"), ("t", "*INQ"), ("send", "Send")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/mymsgs"));
    assert_eq!(
        ran(&b).last().unwrap(),
        "SNDMSG MSG('Lunch at 12?') TOUSR(CWELLMAN) MSGTYPE(*INQ) RPYMSGQ(QUSRSYS/QSECOFR)"
    );
    assert!(ok(&mut b, "/mymsgs").contains("CPC2402"));
    let qs = b.fx.msgs.queues.lock().unwrap();
    assert!(qs["QUSRSYS/CWELLMAN"].iter().any(|m| m.inquiry && m.text == "Lunch at 12?"));
}

#[test]
fn message_detail_and_reply() {
    let mut b = Browser::signed_on();
    let key = key_of(&b, "CPA5305");
    let route = format!("/msgq/QSYS/QSYSOPR/{key}");
    let t = ok(&mut b, &route);
    // The reply starts empty (the default is sent); the button has its
    // own name.
    assert_golden(
        &t,
        "nav/message-detail.html",
        &[
            ("name=\"reply\" value=\"R\"", "name=\"reply\" value=\"\""),
            ("name=\"reply\" value=\"Reply\"", "name=\"send\" value=\"Reply\""),
        ],
    );
    let r = b.post(&route, &[("reply", "r"), ("send", "Reply")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/msgq/QSYS/QSYSOPR"));
    assert_eq!(ran(&b).last().unwrap(), &format!("SNDRPYMSG MSGKEY({key}) MSGQ(QSYS/QSYSOPR) RPY('r') RMV(*NO)"));
    let t = ok(&mut b, "/msgq/QSYS/QSYSOPR");
    assert!(t.contains("CPC2403</a> - Reply R sent"), "{t}");
    // The row is no longer bold and has no Reply.
    assert_eq!(t.matches("<td><b>").count(), 2);
    // A second reply is refused by the command.
    let r = b.post(&route, &[("reply", "C"), ("send", "Reply")]);
    assert_eq!(r.status, 303);
    assert!(ok(&mut b, "/msgq/QSYS/QSYSOPR").contains("CPF2422"));
    // The answered message shows its reply, no form.
    let t = ok(&mut b, &route);
    assert!(t.contains("<tr><th>Reply</th><td><span class=\"mono\">R</span></td></tr>"));
    assert!(!t.contains("name=\"reply\""));
    // An empty reply sends the default.
    let k2 = key_of(&b, "CPA0702");
    b.post(&format!("/msgq/QSYS/QSYSOPR/{k2}"), &[("reply", ""), ("send", "Reply")]);
    assert!(ran(&b).last().unwrap().contains("RPY(*DFT)"));
    let qs = b.fx.msgs.queues.lock().unwrap();
    assert_eq!(qs["QSYS/QSYSOPR"].iter().find(|m| m.key == k2).unwrap().reply.as_deref(), Some("C"));
}

#[test]
fn remove_and_clear_are_confirmed() {
    let mut b = Browser::signed_on();
    let key = key_of(&b, "CPF0907");
    let route = format!("/msgq/QSYS/QSYSOPR/{key}/remove");
    let t = ok(&mut b, &route);
    assert!(t.contains("Confirm Remove Message"));
    assert!(t.contains(&format!("RMVMSG MSGQ(QSYS/QSYSOPR) MSGKEY({key})")));
    assert!(ran(&b).is_empty());
    b.post(&route, &[("ok", "OK")]);
    assert!(ok(&mut b, "/msgq/QSYS/QSYSOPR").contains("40 entries"));
    // Clear keeps the inquiries waiting.
    let r = b.get("/msgq/QSYS/QSYSOPR?clear=Clear+queue...");
    assert_eq!(r.header("Location"), Some("/qs400/navigator/msgq/QSYS/QSYSOPR/clear"));
    assert!(ok(&mut b, "/msgq/QSYS/QSYSOPR/clear").contains("Confirm Clear Message Queue"));
    b.post("/msgq/QSYS/QSYSOPR/clear", &[("clr", "*KEEPUNANS"), ("ok", "OK")]);
    assert_eq!(ran(&b).last().unwrap(), "CLRMSGQ MSGQ(QSYS/QSYSOPR) CLEAR(*KEEPUNANS)");
    assert!(ok(&mut b, "/msgq/QSYS/QSYSOPR").contains("3 entries"));
    // Another user's queue needs the authority.
    let mut w = Browser::as_user("WEBUSER");
    assert_eq!(w.get("/msgq/QUSRSYS/QSECOFR").status, 403);
    assert_eq!(w.get("/mymsgs").status, 200);
}

#[test]
fn history_log() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/qhst");
    let mut fix: Vec<(String, String)> = Vec::new();
    for id in [
        "CPF1164", "CPF1124", "CPA5305", "CPF0907", "CPA4002", "CPF1241", "CPC1129", "CPF2204", "HTP8110", "CPI2209",
        "CPF1107", "CPA0702", "TCP1A03", "CPC3703", "HTP8001", "CPF0927",
    ] {
        fix.push((
            format!("  {id}   "),
            format!("  <a href=\"../shared/help.html\" target=\"_blank\">{id}</a>   "),
        ));
    }
    fix.push(("Subsystem QHTTPSVR active.".into(), format!("Subsystem QHTTPSVR active.\n{NIGHT}")));
    let fix: Vec<(&str, &str)> = fix.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    assert_golden(&t, "nav/history-log.html", &fix);
    // Filters.
    let t = ok(&mut b, "/qhst?id=CPF1107");
    assert!(t.contains("2 entries"));
    let t = ok(&mut b, "/qhst?sev=90");
    assert!(t.contains("3 entries"));
    let t = ok(&mut b, "/qhst?job=INVBATCH");
    assert!(t.contains("2 entries"));
    let t = ok(&mut b, "/qhst?ft=13:00:00&tt=13:59:59");
    assert!(t.contains("4 entries"), "{t}");
    let t = ok(&mut b, "/qhst?page=9");
    assert!(t.contains("Subsystem QBATCH active."));
}

/// The six night entries that fill the golden page's 25 rows (the
/// prototype shows 19).
const NIGHT: &str = "\
05:53:40  <a href=\"../shared/help.html\" target=\"_blank\">CPF1164</a>   00   123330/QPGMR/NIGHT097               Job 123330/QPGMR/NIGHT097 ended on 10/29/26 at 05:53:40; 40 seconds used; end code 0.
05:53:00  <a href=\"../shared/help.html\" target=\"_blank\">CPF1124</a>   00   123330/QPGMR/NIGHT097               Job 123330/QPGMR/NIGHT097 started on 10/29/26 at 05:53:00 in subsystem QBATCH in QSYS.
05:50:00  <a href=\"../shared/help.html\" target=\"_blank\">CPF1164</a>   00   123329/QPGMR/NIGHT096               Job 123329/QPGMR/NIGHT096 ended on 10/29/26 at 05:50:00; 40 seconds used; end code 0.
05:49:20  <a href=\"../shared/help.html\" target=\"_blank\">CPF1124</a>   00   123329/QPGMR/NIGHT096               Job 123329/QPGMR/NIGHT096 started on 10/29/26 at 05:49:20 in subsystem QBATCH in QSYS.
05:46:20  <a href=\"../shared/help.html\" target=\"_blank\">CPF1164</a>   00   123328/QPGMR/NIGHT095               Job 123328/QPGMR/NIGHT095 ended on 10/29/26 at 05:46:20; 40 seconds used; end code 0.
05:45:40  <a href=\"../shared/help.html\" target=\"_blank\">CPF1124</a>   00   123328/QPGMR/NIGHT095               Job 123328/QPGMR/NIGHT095 started on 10/29/26 at 05:45:40 in subsystem QBATCH in QSYS.";
