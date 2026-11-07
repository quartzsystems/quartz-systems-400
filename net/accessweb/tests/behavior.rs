// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sessions, sign-on rules, CSRF, policies, preferences, links: what
//! holds for every page.

mod common;

use std::collections::{BTreeSet, VecDeque};

use accessweb::PRODUCT;
use accessweb::env::Audit;
use accessweb::env::cmd::Cmd as _;
use accessweb::env::policy::{PolicyStore, Setting};
use accessweb::fixture::PASSWORD;
use accessweb::policy;
use common::{Browser, page_links, proto};

#[test]
fn sign_on_rules() {
    let mut b = Browser::new();
    // Not signed on: every page sends you to the sign-on page.
    let r = b.get("/home");
    assert_eq!((r.status, r.header("Location")), (303, Some("/qs400/accessweb/signon")));
    // A wrong password stays on the page with the message and a PW entry.
    let r = b.sign_on("cwellman", "nope");
    assert_eq!(r.status, 200);
    assert!(r.text().contains("CPF22E2"), "{}", r.text());
    assert!(r.text().contains("value=\"CWELLMAN\""));
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Password { code: 'P', .. })));
    // Any profile the system accepts signs on.
    let r = b.sign_on("webuser", PASSWORD);
    assert_eq!((r.status, r.header("Location")), (303, Some("/qs400/accessweb/home")));
    assert!(r.header("Set-Cookie").unwrap().contains("HttpOnly; SameSite=Strict"));
    // The password warning shows once, on the first page.
    let t = b.get("/home").text();
    assert!(t.contains("User: <b>WEBUSER</b>") && t.contains("QAD4031"), "{t}");
    assert!(!b.get("/home").text().contains("QAD4031"));
    // The previous sign-on and where it came from.
    assert!(t.contains("Last sign-on 10/29/26 13:05:00"), "{t}");
    // Sign off ends the session.
    let r = b.get("/signoff");
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/signon"));
    assert_eq!(b.get("/home").status, 303);
}

#[test]
fn sign_on_policy() {
    let mut b = Browser::new();
    b.fx.policy.put("WEBUSERS", &[(policy::SIGNON.into(), Setting::Deny)]).unwrap();
    let r = b.sign_on("WEBUSER", PASSWORD);
    assert_eq!(r.status, 200);
    assert!(r.text().contains("XWA0001"), "{}", r.text());
    assert!(b.cookie.is_none());
    // Others still sign on.
    assert_eq!(b.sign_on("CWELLMAN", PASSWORD).status, 303);
}

#[test]
fn expired_password() {
    let mut b = Browser::new();
    b.fx.system.expired.lock().unwrap().push("WEBUSER".into());
    let r = b.sign_on("WEBUSER", PASSWORD);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/password"));
    let t = b.get("/password").text();
    assert!(t.contains("QAD4022"), "{t}");
    // Nothing else opens.
    assert_eq!(b.get("/home").header("Location"), Some("/qs400/accessweb/password"));
    // Mismatched confirmation: the desktop's message, nothing sent.
    let t = b.post("/password", &[("p0", PASSWORD), ("p1", "NEWPASS1"), ("p2", "NEWPASS2"), ("change", "Change")]).text();
    assert!(t.contains("QAD4027"), "{t}");
    // The system's rules.
    let t = b.post("/password", &[("p0", PASSWORD), ("p1", "SHORT"), ("p2", "SHORT"), ("change", "Change")]).text();
    assert!(t.contains("CPF2234"), "{t}");
    let r = b.post("/password", &[("p0", PASSWORD), ("p1", "NEWPASS1"), ("p2", "NEWPASS1"), ("change", "Change")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/signon?changed=WEBUSER"));
    assert!(b.get("/signon?changed=WEBUSER").text().contains("QAD4023"));
    assert_eq!(b.sign_on("WEBUSER", "NEWPASS1").header("Location"), Some("/qs400/accessweb/home"));
}

#[test]
fn change_password() {
    let mut b = Browser::signed_on();
    let r = b.post("/password", &[("p0", PASSWORD), ("p1", "NEWPASS1"), ("p2", "NEWPASS1"), ("change", "Change")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/home"));
    let t = b.get("/home").text();
    assert!(t.contains("QAD4023</a> - Password changed for CWELLMAN on QSHQ400."), "{t}");
    assert!(!b.get("/password").text().contains("QAD4031"));
    let t = b.post("/password", &[("p0", "wrong"), ("p1", "NEWPASS2"), ("p2", "NEWPASS2"), ("change", "Change")]).text();
    assert!(t.contains("CPF22E2"), "{t}");
}

#[test]
fn posts_need_the_token() {
    let mut b = Browser::signed_on();
    assert_eq!(b.post_raw("/preferences", &[("apply", "Apply")]).status, 403);
    assert_eq!(b.post_raw("/preferences", &[("csrf", "0000"), ("apply", "Apply")]).status, 403);
}

#[test]
fn policies_effective_and_changed() {
    let mut b = Browser::signed_on();
    // WEBUSER inherits WEBUSERS' Deny.
    let t = b.get("/policies?p=WEBUSER").text();
    assert!(t.contains("Effective policies for WEBUSER, including group WEBUSERS."));
    // Deny Print screen for WEBUSER itself: only the change is kept.
    let mut fields: Vec<(String, String)> = Vec::new();
    for s in policy::SECTIONS {
        for (i, (key, _)) in s.functions.iter().enumerate() {
            let denied = accessweb::fixture::policy::WEBUSERS_DENIED.contains(key) || *key == policy::S5250_PRINT;
            fields.push((format!("p_{}_{i}", s.key), if denied { "D" } else { "A" }.into()));
        }
    }
    fields.push(("apply".into(), "Apply".into()));
    let f: Vec<(&str, &str)> = fields.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let r = b.post("/policies?p=WEBUSER", &f);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/policies?p=WEBUSER"));
    assert_eq!(b.fx.policy.settings("WEBUSER"), [(policy::S5250_PRINT.to_string(), Setting::Deny)]);
    let t = b.get("/policies?p=WEBUSER").text();
    assert!(t.contains("XWA0002"), "{t}");
    assert!(t.contains("User WEBUSER"), "{t}");
    // Deny wins: *PUBLIC's Deny reaches CWELLMAN too.
    b.fx.policy.put("*PUBLIC", &[(policy::DB_SQL.into(), Setting::Deny)]).unwrap();
    assert!(!policy::allowed(&b.fx.env, "CWELLMAN", policy::DB_SQL));
    b.fx.policy.put("CWELLMAN", &[(policy::DB_SQL.into(), Setting::Allow)]).unwrap();
    assert!(!policy::allowed(&b.fx.env, "CWELLMAN", policy::DB_SQL));
    // Copy policies from WEBUSER to group WAREHOUSE.
    let r = b.post("/policies/copy?p=WAREHOUSE", &[("from", "WEBUSER (user)"), ("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/policies?p=WAREHOUSE"));
    assert_eq!(b.fx.policy.settings("WAREHOUSE"), b.fx.policy.settings("WEBUSER"));
    // Go picks another profile.
    let r = b.post("/policies", &[("p", "ADMINS (group)")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/policies?p=ADMINS"));
}

#[test]
fn policies_need_secadm() {
    let mut b = Browser::as_user("WEBUSER");
    let r = b.get("/policies");
    assert_eq!(r.status, 403);
    let t = r.text();
    assert!(t.contains("XWA0001") && t.contains("Customize - Change policies (administrator)"), "{t}");
    assert!(t.contains("by the system: it needs special authority"), "{t}");
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Refused { need, .. } if need == "policy policies.change")));
    // Direct POST refused the same way.
    assert_eq!(b.post("/policies?p=WEBUSER", &[("apply", "Apply")]).status, 403);
}

#[test]
fn denied_function_refused_on_post() {
    let mut b = Browser::as_user("WEBUSER");
    b.fx.policy.put("WEBUSERS", &[(policy::PREFS_CHANGE.into(), Setting::Deny)]).unwrap();
    let t = b.get("/preferences").text();
    assert!(t.contains("name=\"apply\" value=\"Apply\" disabled"), "{t}");
    let r = b.post("/preferences", &[("apply", "Apply"), ("rpp", "50")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("XWA0001"));
    assert!(r.text().contains("is set to <b>Deny</b> for group <span class=\"mono\">WEBUSERS</span>."));
}

#[test]
fn preferences_saved_and_used() {
    let mut b = Browser::signed_on();
    let r = b.post(
        "/preferences?tab=database",
        &[("ds", "QGPL"), ("nm", "sys"), ("do", "CSV"), ("mr", "100"), ("apply", "Apply")],
    );
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/preferences?tab=database"));
    let t = b.get("/preferences?tab=database").text();
    assert!(t.contains("XWA0003") && t.contains("<option selected>QGPL</option>"), "{t}");
    assert!(t.contains("value=\"sys\" checked") && !t.contains("Rows per page"), "{t}");
    // A tab's post leaves the others' fields alone.
    let stored = b.fx.prefs.map.lock().unwrap().get("CWELLMAN").cloned().unwrap();
    assert!(stored.contains(&("ss".into(), "27".into())), "{stored:?}");
    // A refused value: nothing saved.
    let t = b.post("/preferences?tab=database", &[("ds", "QGPL"), ("nm", "sql"), ("do", "CSV"), ("mr", "x"), ("apply", "Apply")]).text();
    assert!(t.contains("XWA0004</a> - Value x is not valid for Maximum rows returned."), "{t}");
    // Dates as the user chose them.
    b.post(
        "/preferences",
        &[("rpp", "25"), ("df", "YYYY-MM-DD"), ("tf", "HH:MM:SS (24 hour)"), ("tz", "UTC"), ("lang", "English (US)"), ("h", "win"), ("ds", "QGPL"), ("nm", "sql"), ("do", "HTML"), ("mr", "500"), ("ss", "24"), ("cl", "g"), ("fs", "10pt"), ("cu", "Underline"), ("hl", "compact"), ("ok", "OK")],
    );
    let t = b.get("/home").text();
    assert!(t.contains("Last sign-on 2026-10-29 17:05:00"), "{t}");
    // Compact, no counts (the box was not sent).
    assert!(!t.contains("class=\"home\"") && !t.contains("spooled files"), "{t}");
    // Restore defaults.
    b.post("/preferences", &[("restore_defaults", "Restore defaults")]);
    let stored = b.fx.prefs.map.lock().unwrap().get("CWELLMAN").cloned().unwrap();
    assert_eq!(stored, [("_from".to_string(), "10.0.10.55".to_string())]);
}

#[test]
fn home_follows_policies() {
    let mut b = Browser::as_user("WEBUSER");
    b.fx.policy.put("WEBUSERS", &[(policy::DB_BROWSE.into(), Setting::Deny)]).unwrap();
    let t = b.get("/home").text();
    assert!(!t.contains(">Database</a><br>") && t.contains(">Files</a><br>"), "{t}");
}

#[test]
fn messages_and_help_resolve() {
    let mut b = Browser::signed_on();
    let r = b.get("/msg/XWA0001?d=Jobs%20-%20End%20server%20jobs&d=WEBUSER");
    assert_eq!(r.status, 200);
    let t = r.text();
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(at("<h2>Message</h2>") < at("<h2>Cause</h2>") && at("<h2>Cause</h2>") < at("<h2>Recovery</h2>"));
    assert!(t.contains("Function Jobs - End server jobs is not allowed for user profile WEBUSER"));
    assert_eq!(b.get("/msg/CPF9999").status, 404);
    assert!(b.get("/help/policies").text().contains("<title>Help: Policies - QS/400</title>"));
    assert!(b.get("/help/nothing").text().contains("Contents"));
    assert_eq!(b.get(qsweb::frame::CSS_PATH).bytes(), qsweb::CSS);
}

#[test]
fn areas_not_built_say_so() {
    let mut b = Browser::signed_on();
    // The first area still to come.
    let Some(route) = PENDING.first().map(|p| p.1[0]) else { return };
    let r = b.get(route);
    assert_eq!(r.status, 501);
    assert!(r.text().contains("XWA0099"), "{}", r.text());
}

/// Routes not built yet, by area: each area's agent removes its own.
const PENDING: &[(&str, &[&str])] = &[
];

/// Every link and form on every reachable page resolves to a page of
/// this product (or the pages of others it links to).
#[test]
fn crawl() {
    let mut b = Browser::signed_on();
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([PRODUCT.url("/home")]);
    let mut broken = Vec::new();
    while let Some(url) = queue.pop_front() {
        if !seen.insert(url.clone())
            || url.ends_with("/signoff")
            || url.contains("/qs400/navigator/")
            || url.contains("/qs400/httpadmin/")
        {
            continue;
        }
        let r = b.get(&url);
        // The tree's [+] redirects.
        if r.status == 303
            && let Some(to) = r.header("Location").filter(|l| l.starts_with(PRODUCT.prefix))
        {
            queue.push_back(to.to_string());
            continue;
        }
        // Ending a server job goes on to Navigator's confirmation.
        if r.status == 303 && r.header("Location").is_some_and(|l| l.contains("/qs400/navigator/")) {
            continue;
        }
        if r.status != 200 {
            let route = url.strip_prefix(PRODUCT.prefix).unwrap_or(&url);
            if PENDING.iter().flat_map(|p| p.1.iter()).any(|p| route.starts_with(p)) {
                continue;
            }
            broken.push(format!("{url}: {}", r.status));
            continue;
        }
        if url == qsweb::frame::CSS_PATH {
            continue;
        }
        let page = url.split('?').next().unwrap_or("").to_string();
        for (method, href) in page_links(&r.text()) {
            let abs = if href.is_empty() {
                url.clone()
            } else if href.starts_with('?') {
                format!("{page}{href}")
            } else {
                href.clone()
            };
            if abs == "#" {
                continue;
            }
            assert!(proto(&abs).is_some() || abs == qsweb::frame::CSS_PATH, "{url} links to {abs}");
            if method == "GET" {
                queue.push_back(abs);
            }
        }
    }
    assert!(broken.is_empty(), "broken links: {broken:#?}");
    assert!(seen.len() > 5, "{seen:?}");
}

/// No other vendor's product or feature names in the sources or pages.
#[test]
fn no_ibm_wording() {
    let words = [
        "IBM", "iSeries", "System i", "AS/400", "OS/400", "i5/OS", "WebSphere", "Navigator for i", "Access Client Solutions",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    for dir in ["src", "tests/behavior.rs", "tests/golden.rs", "tests/common", "tests/golden/web"] {
        for f in walk(&root.join(dir)) {
            let t = std::fs::read_to_string(&f).unwrap_or_default();
            for (n, line) in t.lines().enumerate() {
                if line.contains("let words") || line.contains("\"IBM\", \"iSeries\"") {
                    continue;
                }
                for w in words {
                    if line.contains(w) {
                        hits.push(format!("{}:{}: {w}", f.display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(hits.is_empty(), "{hits:#?}");
}

fn walk(d: &std::path::Path) -> Vec<std::path::PathBuf> {
    if d.is_file() {
        return vec![d.to_path_buf()];
    }
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(d) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

// Messages, jobs, commands, download (handoff steps 16-21).

fn ran(v: &std::sync::Mutex<Vec<String>>) -> Vec<String> {
    v.lock().unwrap().clone()
}

#[test]
fn messages_reply_send_remove() {
    let mut b = Browser::signed_on();
    // An empty reply sends the inquiry's default, as the user.
    let r = b.post("/messages", &[("key", "00000003"), ("r", ""), ("reply", "Reply")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/messages"));
    assert_eq!(
        ran(&b.fx.msgs.ran),
        ["CWELLMAN: SNDRPYMSG MSGKEY(00000003) MSGQ(QUSRSYS/CWELLMAN) RPY(*DFT) RMV(*NO)"]
    );
    let t = b.get("/messages").text();
    assert!(t.contains("QAD9002</a> - Reply sent to CPA0702."), "{t}");
    assert!(t.contains("Reply: C</span>") && !t.contains("<b>CPF0001 received"), "{t}");
    // Send to a user; an inquiry's reply comes to the user's queue.
    b.post("/messages", &[("to", "user"), ("tou", "cwellman"), ("m", "It's done"), ("i", "yes"), ("send", "Send")]);
    assert_eq!(
        ran(&b.fx.msgs.ran)[1],
        "CWELLMAN: SNDMSG MSG('It''s done') TOUSR(CWELLMAN) MSGTYPE(*INQ) RPYMSGQ(QUSRSYS/CWELLMAN)"
    );
    assert!(b.get("/messages").text().contains("QAD9004</a> - Message sent to CWELLMAN."));
    // Remove of an informational message asks once, on the page.
    let t = b.get("/messages?remove=00000002").text();
    assert!(t.contains("name=\"remove\" value=\"00000002\""), "{t}");
    b.post("/messages", &[("remove", "00000002"), ("ok", "Remove")]);
    assert_eq!(ran(&b.fx.msgs.ran)[2], "CWELLMAN: RMVMSG MSGQ(QUSRSYS/CWELLMAN) MSGKEY(00000002)");
    assert!(b.get("/messages").text().contains("QAD9003</a> - 1 messages removed from QUSRSYS/CWELLMAN."));
}

#[test]
fn removing_an_unanswered_inquiry_asks_twice() {
    let mut b = Browser::signed_on();
    let t = b.get("/messages").text();
    assert!(t.contains("/qs400/accessweb/messages/00000003/confirm"), "{t}");
    assert!(b.get("/messages/00000003/confirm").text().contains("Press OK to remove the inquiry"));
    // The first OK asks again; nothing is removed.
    let t = b.post("/messages/00000003/confirm", &[("ok", "OK")]).text();
    assert!(t.contains("Press OK again") && t.contains("name=\"again\""), "{t}");
    assert!(ran(&b.fx.msgs.ran).is_empty());
    // A Remove posted from the list goes to the same two questions.
    let r = b.post("/messages", &[("remove", "00000003"), ("ok", "Remove")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/messages/00000003/confirm"));
    let r = b.post("/messages/00000003/confirm", &[("again", "1"), ("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/messages"));
    assert_eq!(ran(&b.fx.msgs.ran), ["CWELLMAN: RMVMSG MSGQ(QUSRSYS/CWELLMAN) MSGKEY(00000003)"]);
}

#[test]
fn messages_follow_policies() {
    let mut b = Browser::as_user("WEBUSER");
    b.fx.policy.put("WEBUSERS", &[(policy::MSGS_SEND.into(), Setting::Deny)]).unwrap();
    let t = b.get("/messages").text();
    assert!(!t.contains("name=\"send\""), "{t}");
    let r = b.post("/messages", &[("to", "user"), ("tou", "QSECOFR"), ("m", "hi"), ("send", "Send")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("XWA0001"));
    assert!(ran(&b.fx.msgs.ran).is_empty());
    assert_eq!(b.get("/messages/send").status, 403);
    b.fx.policy.put("WEBUSERS", &[(policy::MSGS_DISPLAY.into(), Setting::Deny)]).unwrap();
    assert_eq!(b.get("/messages").status, 403);
}

#[test]
fn a_message_arriving_is_watched() {
    let mut b = Browser::signed_on();
    assert!(!b.get("/home").text().contains("Messages waiting:"));
    // Another job sends an inquiry to CWELLMAN.
    let key = b.fx.msgs.arrive(
        "QUSRSYS/CWELLMAN",
        accessweb::env::msgs::QueuedMessage {
            id: "CPA5305".into(),
            severity: 99,
            inquiry: true,
            sent: accessweb::fixture::now(),
            text: "Record 12 of member ASSETS is in use. (C I)".into(),
            job: "123540/CWELLMAN/INVLOAD2".into(),
            user: "CWELLMAN".into(),
            default: "C".into(),
            ..Default::default()
        },
    );
    // The next page asked for is Message Arrived, back to it after.
    let r = b.get("/home");
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/messages/arrived?back=/qs400/accessweb/home"));
    let t = b.get("/messages/arrived?back=%2Fqs400%2Faccessweb%2Fhome").text();
    assert!(t.contains("CPA5305</a> - Inquiry message from job 123540/CWELLMAN/INVLOAD2 arrived at 14:03:40."), "{t}");
    assert!(t.contains("Messages waiting: <b>1</b>"), "{t}");
    // Later: back where the user was, still waiting in the header.
    let r = b.post(
        "/messages/arrived?back=%2Fqs400%2Faccessweb%2Fhome",
        &[("key", &key), ("queue", "QUSRSYS/CWELLMAN"), ("later", "Later")],
    );
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/home"));
    let r = b.get("/home");
    assert_eq!(r.status, 200);
    assert!(r.text().contains("Messages waiting: <b>1</b>"), "{}", r.text());
    // Looking at the queue clears it.
    b.get("/messages");
    assert!(!b.get("/home").text().contains("Messages waiting:"));
    // Watch Off: nothing more is watched.
    b.get("/messages?w=Off");
    b.fx.msgs.arrive(
        "QUSRSYS/CWELLMAN",
        accessweb::env::msgs::QueuedMessage {
            inquiry: true,
            text: "x".into(),
            ..Default::default()
        },
    );
    assert_eq!(b.get("/home").status, 200);
}

#[test]
fn jobs_hold_release_end() {
    let mut b = Browser::signed_on();
    let t = b.get("/jobs?hold=123488/CWELLMAN/QPADEV0003").text();
    assert!(t.contains("HLDJOB JOB(123488/CWELLMAN/QPADEV0003)"), "{t}");
    assert!(ran(&b.fx.jobs.ran).is_empty());
    let r = b.post("/jobs", &[("job", "123488/CWELLMAN/QPADEV0003"), ("op", "hold"), ("ok", "Hold")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/jobs"));
    b.post("/jobs", &[("job", "123402/CWELLMAN/INVBATCH"), ("op", "release"), ("ok", "Release")]);
    assert_eq!(
        ran(&b.fx.jobs.ran),
        ["CWELLMAN: HLDJOB JOB(123488/CWELLMAN/QPADEV0003)", "CWELLMAN: RLSJOB JOB(123402/CWELLMAN/INVBATCH)"]
    );
    assert!(b.get("/jobs").text().contains("QAD5010</a> - Command RLSJOB completed."));
    // The Status filter.
    let t = b.get("/jobs?s=Completed+(output)").text();
    assert!(!t.contains("123488") && t.contains("123311"), "{t}");
    // End of a server job: *JOBCTL first, then Navigator's confirmation.
    let r = b.get("/jobs/server/123420/QUSER/QZRCSRVS/end");
    assert_eq!(r.header("Location"), Some("/qs400/navigator/job/123420/QUSER/QZRCSRVS/end"));
    let mut w = Browser::as_user("WEBUSER");
    // WEBUSERS denies it by default: allowed, the special authority still decides.
    w.fx.policy.put("WEBUSERS", &[]).unwrap();
    let r = w.get("/jobs/server/123420/QUSER/QZRCSRVS/end");
    assert_eq!(r.status, 403);
    assert!(r.text().contains("CPF2207") && r.text().contains("*JOBCTL"));
    w.fx.policy.put("WEBUSERS", &[(policy::JOBS_END_SERVER.into(), Setting::Deny)]).unwrap();
    assert!(!w.get("/jobs/server").text().contains(">End</a>"));
    assert!(w.get("/jobs/server/123420/QUSER/QZRCSRVS/end").text().contains("XWA0001"));
    w.fx.policy.put("WEBUSERS", &[(policy::JOBS_END.into(), Setting::Deny)]).unwrap();
    assert!(!w.get("/jobs").text().contains(">End</a>"));
}

#[test]
fn navigator_links_follow_the_host() {
    let mut b = Browser::signed_on();
    let mut req = qsweb::http::Request::new("GET", "/qs400/accessweb/jobs");
    req.headers.push(("host".into(), "qshq400.example:2003".into()));
    let t = b.send(req).text();
    assert!(t.contains("http://qshq400.example:2002/qs400/navigator/job/123488/CWELLMAN/QPADEV0003/joblog"), "{t}");
}

#[test]
fn commands_run_check_submit() {
    let mut b = Browser::signed_on();
    let r = b.post("/cmd", &[("cmd", "WRKACTJOB SBS(QINTER)"), ("how", "int"), ("run", "Run")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/cmd?c=WRKACTJOB%20SBS%28QINTER%29&how=int"));
    assert_eq!(ran(&b.fx.cmd.ran), ["CWELLMAN: WRKACTJOB SBS(QINTER)"]);
    let t = b.get("/cmd?c=WRKACTJOB+SBS%28QINTER%29&how=int").text();
    assert!(t.contains("QAD5010</a> - Command WRKACTJOB completed.") && t.contains("value=\"WRKACTJOB SBS(QINTER)\""), "{t}");
    // Newest first in the recent commands.
    assert_eq!(b.fx.cmd.recent("CWELLMAN")[0], "WRKACTJOB SBS(QINTER)");
    // Batch: SBMJOB to the queue chosen; the job it made.
    b.post("/cmd", &[("cmd", "DSPMSG QSYSOPR"), ("how", "batch"), ("jobq", "QGPL/QPGMR"), ("run", "Run")]);
    assert_eq!(ran(&b.fx.cmd.ran)[1], "CWELLMAN: SBMJOB CMD(DSPMSG QSYSOPR) JOBQ(QGPL/QPGMR)");
    let t = b.get("/cmd").text();
    assert!(t.contains("XWA0050</a> - Command submitted to job queue QGPL/QPGMR as job 123540/CWELLMAN/QDFTJOBD."), "{t}");
    // Check, and a command that is not there.
    b.post("/cmd", &[("cmd", "DSPMSG"), ("check", "Check")]);
    b.post("/cmd", &[("cmd", "NOSUCH X"), ("run", "Run")]);
    let t = b.get("/cmd").text();
    assert!(t.contains("XWA0051</a> - Command DSPMSG checked; no errors found."), "{t}");
    assert!(t.contains("QAD5013</a> - Command NOSUCH not found.") && t.contains("CPD0030"), "{t}");
    // Clear empties the results; the recent commands stay.
    b.post("/cmd", &[("clear", "Clear")]);
    let t = b.get("/cmd").text();
    assert!(!t.contains("<h2>Results</h2>") && t.contains("Recent commands"), "{t}");
    // Denied: refused on POST with XWA0001.
    let mut w = Browser::as_user("WEBUSER");
    w.fx.policy.put("WEBUSERS", &[(policy::CMD_RUN.into(), Setting::Deny)]).unwrap();
    let r = w.post("/cmd", &[("cmd", "DSPMSG"), ("run", "Run")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("XWA0001") && ran(&w.fx.cmd.ran).is_empty());
}

#[test]
fn prompt_builds_and_runs() {
    let mut b = Browser::signed_on();
    // Prompt from Run command: the page for the typed string.
    let r = b.post("/cmd/prompt", &[("cmd", "WRKACTJOB SBS(QHTTPSVR QBATCH)"), ("prompt", "Prompt")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/cmd/prompt?cmd=WRKACTJOB%20SBS%28QHTTPSVR%20QBATCH%29"));
    let t = b.get("/cmd/prompt?cmd=WRKACTJOB+SBS%28QHTTPSVR+QBATCH%29").text();
    assert!(t.contains("name=\"sbs2\" value=\"QBATCH\"") && t.contains("name=\"sbs3\" value=\"\""), "{t}");
    // + adds a field.
    assert!(b.get("/cmd/prompt?cmd=WRKACTJOB&n.SBS=4").text().contains("name=\"sbs4\""));
    // Show command string from the fields.
    let at = "/cmd/prompt?cmd=WRKACTJOB";
    let fields = [
        ("output", "*PRINT"),
        ("reset", "*NO"),
        ("sbs1", "QINTER"),
        ("sbs2", ""),
        ("cpupctlmt", "*NONE"),
        ("rsplmt", ""),
        ("seq", "*JOB"),
        ("job", "QPADEV*"),
        ("interval", ""),
    ];
    let mut f = fields.to_vec();
    f.push(("show", "Show command string"));
    let t = b.post(at, &f).text();
    assert!(t.contains("WRKACTJOB OUTPUT(*PRINT) RESET(*NO) SBS(QINTER) CPUPCTLMT(*NONE) SEQ(*JOB) JOB(QPADEV*)"), "{t}");
    assert!(t.contains("<option selected>*JOB</option>"), "{t}");
    // Run it.
    let mut f = fields.to_vec();
    f.push(("run", "Run"));
    b.post(at, &f);
    assert_eq!(
        ran(&b.fx.cmd.ran),
        ["CWELLMAN: WRKACTJOB OUTPUT(*PRINT) RESET(*NO) SBS(QINTER) CPUPCTLMT(*NONE) SEQ(*JOB) JOB(QPADEV*)"]
    );
    // A command that is not there.
    let r = b.get("/cmd/prompt?cmd=NOSUCH");
    assert_eq!(r.status, 303);
    assert!(b.get("/cmd?c=NOSUCH").text().contains("CPD0030"));
}

#[test]
fn my_commands_save_share_delete() {
    let mut b = Browser::signed_on();
    let r = b.post("/cmd", &[("cmd", "DSPJOBLOG"), ("how", "int"), ("save", "Save to My commands")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/cmd/saved"));
    let t = b.get("/cmd/saved").text();
    assert!(t.contains("XWA0052</a> - Command DSPJOBLOG saved in My commands.") && t.contains(">DSPJOBLOG</span>"), "{t}");
    // Edit replaces the command it was opened for.
    assert!(b.get("/cmd?edit=0").text().contains("value=\"WRKACTJOB SBS(QHTTPSVR)\""));
    b.post("/cmd?edit=0", &[("cmd", "WRKACTJOB SBS(QINTER)"), ("how", "int"), ("save", "Save to My commands")]);
    assert_eq!(b.fx.cmd.saved("CWELLMAN")[0].command, "WRKACTJOB SBS(QINTER)");
    // Share with one of the user's groups only.
    assert!(b.get("/cmd/saved?share=5").text().contains("<option>ADMINS</option>"));
    let r = b.post("/cmd/saved", &[("n", "5"), ("name", "Job log"), ("to", "WAREHOUSE"), ("ok", "Share")]);
    assert_eq!(r.status, 303);
    assert!(b.get("/cmd/saved").text().contains("XWA0004"));
    b.post("/cmd/saved", &[("n", "5"), ("name", "Job log"), ("to", "ADMINS"), ("ok", "Share")]);
    let kept = b.fx.cmd.saved("CWELLMAN")[5].clone();
    assert_eq!((kept.name.as_str(), kept.share.as_str()), ("Job log", "ADMINS"));
    // Another user sees what is shared with *PUBLIC.
    let mut w = Browser::as_user("WEBUSER");
    w.fx.cmd.put_saved("CWELLMAN", &b.fx.cmd.saved("CWELLMAN")).unwrap();
    let t = w.get("/cmd/saved").text();
    assert!(t.contains("Shared with you") && t.contains("DSPMSG MSGQ(QSYSOPR)") && !t.contains("Job log"), "{t}");
    // Delete asks first.
    assert!(b.get("/cmd/saved/5/confirm").text().contains("Press OK to delete the saved command"));
    b.post("/cmd/saved/5/confirm", &[("ok", "OK")]);
    assert_eq!(b.fx.cmd.saved("CWELLMAN").len(), 5);
}

#[test]
fn download_sends_files() {
    let mut b = Browser::signed_on();
    let r = b.get("/download/connection.txt");
    assert_eq!(r.status, 200);
    assert_eq!(r.header("Content-Type"), Some("text/plain; charset=utf-8"));
    assert!(r.text().contains("ODBC / JDBC connection details"));
    assert_eq!(b.get("/download/nothing.bin").status, 303);
    assert_eq!(b.get("/download/..").status, 404);
}
