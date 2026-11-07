// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Served pages against the golden pages, with the fixture's data.
//! Golden corrections (`fix` arguments) are listed in docs/ACCESSWEB.md.

mod common;

use common::{Browser, HEADER_USER, assert_golden, golden, proto};

/// Every route of the handoff's build order and the golden page it
/// renders. Each area's steps make its pages match; the map holds now.
const ROUTES: &[(&str, &str)] = &[
    // Group 1.
    ("/signon", "web/signon.html"),
    ("/password", "web/change-password.html"),
    ("/policies?p=WEBUSER", "web/policies.html"),
    ("/home", "web/home.html"),
    ("/preferences", "web/preferences.html"),
    // Group 2.
    ("/5250", "web/5250-config.html"),
    ("/5250/1/properties", "web/5250-properties.html"),
    ("/5250/verify", "web/5250-verify.html"),
    ("/5250/session/1", "web/5250-session.html"),
    ("/5250/session/1?scale=2", "web/5250-session-x2.html"),
    ("/5250/session/1/screen.png", "shared/img/5250-24x80.png"),
    ("/5250/session/1/key", "web/5250-session.html"),
    ("/5250/1/keymap", "web/5250-keymap.html"),
    ("/5250/1/macros", "web/5250-macros.html"),
    // Group 3.
    ("/db/tables?s=HLINV", "web/tables.html"),
    ("/db/HLINV/ASSETS", "web/table-view.html"),
    ("/db/HLINV/ASSETS/insert", "web/table-insert.html"),
    ("/db/HLINV/ASSETS/edit?key=QS000101", "web/table-insert.html"),
    ("/sql", "web/run-sql.html"),
    ("/sql/result/2", "web/run-sql-result-2.html"),
    ("/requests", "web/my-requests.html"),
    ("/db/copy/to", "web/copy-import.html"),
    ("/db/copy/from", "web/copy-export.html"),
    // Group 4.
    ("/files?path=/home/CWELLMAN", "web/files.html"),
    ("/files/props?path=/home/CWELLMAN/assets_br1.csv", "web/file-props.html"),
    ("/files/shares", "web/my-files.html"),
    ("/files/edit?path=/home/CWELLMAN/invload.clle", "web/file-edit.html"),
    // Group 5.
    ("/print", "web/printer-output.html"),
    ("/print/props/1", "web/splf-props.html"),
    ("/print/printers", "web/printers.html"),
    ("/messages", "web/messages.html"),
    ("/messages/arrived", "web/message-arrived.html"),
    ("/messages/send", "web/send-message.html"),
    ("/jobs", "web/my-jobs.html"),
    ("/jobs/server", "web/server-jobs.html"),
    // Group 6.
    ("/cmd", "web/run-command.html"),
    ("/cmd/prompt?cmd=WRKACTJOB", "web/command-prompt.html"),
    ("/cmd/saved", "web/my-commands.html"),
    ("/download", "web/download.html"),
    // Pages of the other products it links to.
    ("/qs400/navigator/jobs", "nav/active-jobs.html"),
    ("/qs400/navigator/job/123402/CWELLMAN/INVBATCH/end", "nav/end-job-confirm.html"),
    ("/qs400/navigator/job/123402/CWELLMAN/INVBATCH/general", "nav/job-props-general.html"),
    ("/qs400/navigator/job/123402/CWELLMAN/INVBATCH/joblog", "nav/job-props-joblog.html"),
    ("/qs400/navigator/msgq/QUSRSYS/CWELLMAN/1", "nav/message-detail.html"),
    ("/qs400/navigator/splf/123402/CWELLMAN/INVBATCH/INVRPT/1", "nav/spooled-view.html"),
    ("/qs400/navigator/db/HLINV/ASSETS/columns", "nav/table-columns.html"),
    ("/qs400/httpadmin/instance/ACCESSWEB/general", "http/instance-general.html"),
];

/// Goldens that are another state of a page above (the same route
/// with other data): file and the page whose route renders it.
const STATES: &[(&str, &str)] = &[
    ("web/5250-session-white.html", "web/5250-session.html"),
    ("web/5250-session-wide.html", "web/5250-session.html"),
    ("web/5250-session-disconnected.html", "web/5250-session.html"),
    ("web/5250-session-wait.html", "web/5250-session.html"),
    ("shared/img/5250-24x80-off.png", "shared/img/5250-24x80.png"),
    ("shared/img/5250-24x80-white.png", "shared/img/5250-24x80.png"),
    ("shared/img/5250-24x80-x2.png", "shared/img/5250-24x80.png"),
    ("shared/img/5250-27x132.png", "shared/img/5250-24x80.png"),
    ("shared/img/5250-signon.png", "shared/img/5250-24x80.png"),
    ("shared/img/5250-wait.png", "shared/img/5250-24x80.png"),
];

#[test]
fn every_route_maps_to_its_golden() {
    let url = |r: &str| if r.starts_with("/qs400/") { r.to_string() } else { accessweb::PRODUCT.url(r) };
    for (route, file) in ROUTES {
        assert_eq!(proto(&url(route)).as_deref(), Some(*file), "{route}");
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut seen = Vec::new();
    for dir in ["web", "nav", "http", "shared/img"] {
        for e in std::fs::read_dir(root.join(dir)).unwrap().flatten() {
            let file = format!("{dir}/{}", e.file_name().to_string_lossy());
            let covered = ROUTES.iter().any(|r| r.1 == file) || STATES.iter().any(|s| s.0 == file);
            assert!(covered, "no route renders {file}");
            seen.push(file);
        }
    }
    for (file, base) in STATES {
        assert!(seen.contains(&(*file).to_string()) && ROUTES.iter().any(|r| r.1 == *base), "{file}");
    }
    assert_eq!(seen.iter().filter(|f| f.starts_with("web/")).count(), 40, "{seen:?}");
}

#[test]
fn signon() {
    let mut b = Browser::new();
    let r = b.get("/signon");
    assert_eq!(r.status, 200);
    // The form posts back to the sign-on route; home follows by redirect.
    assert_golden(&r.text(), "web/signon.html", &[("action=\"home.html\"", "action=\"signon.html\"")]);
}

#[test]
fn change_password() {
    let mut b = Browser::signed_on();
    let r = b.get("/password");
    assert_eq!(r.status, 200);
    assert_golden(
        &r.text(),
        "web/change-password.html",
        &[
            HEADER_USER,
            // The page's own item is current, not Preferences.
            (
                "<a href=\"preferences.html\" class=\"navcur\">Preferences</a>",
                "<a href=\"preferences.html\">Preferences</a>",
            ),
            (
                "<a href=\"change-password.html\">Change password</a>",
                "<a href=\"change-password.html\" class=\"navcur\">Change password</a>",
            ),
            // The desktop's warning, not a sign-on error's ID.
            (
                "CPF1107</a> - Password for user profile CWELLMAN expires in 5 days.",
                "QAD4031</a> - Password for CWELLMAN on QSHQ400 expires in 5 days.",
            ),
            // One form, not two nested.
            ("<form method=\"post\"><form method=\"post\">", "<form method=\"post\">"),
            ("</form></form>", "</form>"),
        ],
    );
}

#[test]
fn home() {
    let mut b = Browser::signed_on();
    let r = b.get("/home");
    assert_eq!(r.status, 200);
    assert_golden(&r.text(), "web/home.html", &[HEADER_USER]);
}

/// The Sign on function's row, ahead of 5250 (two rows: the stripes of
/// the rest stay as they are).
const SIGNON_ROWS: &str = "<tr>\n<td><b>Sign on</b></td>\n<td class=\"radio\"></td>\n<td class=\"radio\"></td>\n<td></td>\n</tr>\n<tr class=\"alt\">\n<td>Sign on to Access for Web</td>\n<td class=\"radio\"><input type=\"radio\" name=\"p_signon_0\" value=\"A\" checked> </td>\n<td class=\"radio\"><input type=\"radio\" name=\"p_signon_0\" value=\"D\"> </td>\n<td><span class=\"small grey\">Default</span></td>\n</tr>\n<tr>\n<td><b>5250</b></td>";

#[test]
fn policies() {
    let mut b = Browser::signed_on();
    let r = b.get("/policies");
    assert_eq!(r.status, 200);
    assert_golden(
        &r.text(),
        "web/policies.html",
        &[
            HEADER_USER,
            ("<tr>\n<td><b>5250</b></td>", SIGNON_ROWS),
            // Four nameless buttons in one form cannot be told apart.
            (
                "<input type=\"submit\" value=\"Apply\"><input type=\"submit\" value=\"OK\"><input type=\"submit\" value=\"Cancel\"><input type=\"submit\" value=\"Copy policies from...\">",
                "<input type=\"submit\" name=\"apply\" value=\"Apply\"><input type=\"submit\" name=\"ok\" value=\"OK\"><input type=\"submit\" name=\"cancel\" value=\"Cancel\"><input type=\"submit\" name=\"copy\" value=\"Copy policies from...\">",
            ),
        ],
    );
    assert!(golden("web/policies.html").contains("<option>WEBUSER (user)</option>"));
}

#[test]
fn preferences() {
    let mut b = Browser::signed_on();
    let r = b.get("/preferences");
    assert_eq!(r.status, 200);
    assert_golden(
        &r.text(),
        "web/preferences.html",
        &[HEADER_USER],
    );
}
