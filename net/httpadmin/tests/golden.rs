// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Served pages against the golden pages, with the fixture's data.
//! Golden corrections (`fix` arguments) are listed in docs/HTTPADMIN.md.

mod common;

use common::{Browser, assert_golden};
use httpadmin::PRODUCT;
use qsweb::frame::{Ctx, NavGroup, Page, Product, render};
use qsweb::msg::{Kind, Message};
use qsweb::partials::{
    Button, Cell, Col, FilterField, List, Paging, Prop, PropGroup, buttons, filter, form, list_table, paging, props,
    props_table, tabs, wizard,
};

#[test]
fn signon() {
    let mut b = Browser::new();
    let r = b.get("/signon");
    assert_eq!(r.status, 200);
    // The form posts back to the sign-on route, not to the list.
    assert_golden(&r.text(), "http/signon.html", &[("action=\"instances.html\"", "action=\"signon.html\"")]);
}

#[test]
fn instances() {
    let mut b = Browser::signed_on();
    let id = b.cookie.clone().unwrap();
    b.admin.sessions.flash(
        &id,
        httpadmin::msgs::message(Kind::Done, "HTP8001", &["API", "*:9443"]),
    );
    let r = b.get("/instances");
    assert_eq!(r.status, 200);
    assert_golden(&r.text(), "http/instances.html", &[]);
}

/// The pattern reference, rebuilt from the partials alone.
#[test]
#[allow(clippy::too_many_lines)]
fn components() {
    static P: Product = Product {
        wordmark: "QS/400",
        crumb_root: "QS/400",
        prefix: "/qs400/httpadmin",
        nav: httpadmin::NAV,
        system_label: "System",
    };
    let _: &[NavGroup] = P.nav;
    let ctx = Ctx {
        product: &P,
        system: "QSHQ400".into(),
        user: "QSECOFR".into(),
        csrf: "t".into(),
    };
    let nav = "/qs400/navigator";
    let act = format!(
        "<a href=\"{nav}/jobs\">Hold</a> | <a href=\"{nav}/job/123456/QTMHHTTP/ADMIN/end\">End</a> | <a href=\"{nav}/job/123456/QTMHHTTP/ADMIN/general\">Properties</a>"
    );
    let jobs = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Job name", "name"),
            Col::sorted("User", "user"),
            Col::sorted("Number", "number"),
            Col::sorted("Status", "status"),
            Col::sorted("CPU %", "cpu"),
        ],
        rows: vec![
            vec![
                Cell::act(act.clone()),
                Cell::mono("ADMIN"),
                Cell::mono("QTMHHTTP"),
                Cell::mono("123456"),
                Cell::html("<span class=\"ok\">RUN</span>".into()),
                Cell::num("0.3"),
            ],
            vec![
                Cell::act(act),
                Cell::mono("QPADEV0001"),
                Cell::mono("QSECOFR"),
                Cell::mono("123460"),
                Cell::html("<span class=\"err\">MSGW</span>".into()),
                Cell::num("0.0"),
            ],
        ],
        empty: String::new(),
        keep: Vec::new(),
    };
    let empty = List {
        cols: ["Job name", "User", "Number", "Status"].iter().map(|c| Col::plain(c)).collect(),
        rows: Vec::new(),
        empty: "No active jobs match the filter.".into(),
        keep: Vec::new(),
    };
    let tab = |l: &'static str| (l, PRODUCT.url("/help/components"));
    let props_body = props(
        &ctx,
        &[
            PropGroup {
                title: Some("Identification".into()),
                rows: vec![
                    Prop::new("Instance name", "<span class=\"mono\">WEBSTORE</span>".into()),
                    Prop::new("Description", qsweb::partials::input("desc", "Web store front end", 40)).help("desc"),
                ],
            },
            PropGroup {
                title: Some("Startup".into()),
                rows: vec![
                    Prop::new("Start with STRTCP", qsweb::partials::checkbox("autostart", true)),
                    Prop::new("Server user profile", qsweb::partials::input("usrprf", "QTMHHTTP", 10)),
                ],
            },
        ],
        &[Button::new("apply", "Apply"), Button::new("ok", "OK"), Button::new("cancel", "Cancel")],
    );
    let wiz_body = format!(
        "<h2>Server root</h2><p class=\"small grey\">The directory that holds the configuration, logs and content of the instance.</p>{}",
        props_table(
            &P,
            &[PropGroup {
                title: None,
                rows: vec![Prop::new("Server root", qsweb::partials::input_cmd("root", "/www/webstore", 40))],
            }],
            false
        )
    );
    let controls = "<table class=\"props\" cellspacing=\"0\">\n<tr><th>Text</th><td><input type=\"text\" name=\"t\" value=\"QGPL\" size=\"10\"> &nbsp; Command string: <input type=\"text\" name=\"c\" value=\"WRKACTJOB SBS(QHTTPSVR)\" size=\"40\" class=\"cmd\"></td></tr>\n<tr><th>Select</th><td><select name=\"s\"><option>*ALL</option><option>*SYSTEM</option><option>*OBJECTSIGNING</option></select></td></tr>\n<tr><th>Checkbox / radio</th><td><input type=\"checkbox\" name=\"a\" checked> Start automatically &nbsp; <input type=\"radio\" name=\"r\" value=\"*CNTRLD\" checked> *CNTRLD <input type=\"radio\" name=\"r\" value=\"*IMMED\"> *IMMED</td></tr>\n<tr><th>File</th><td><input type=\"file\" name=\"f\"> <input type=\"submit\" value=\"Browse...\"></td></tr>\n<tr><th>Textarea</th><td><textarea name=\"x\" rows=\"3\" cols=\"60\" class=\"conf\">Listen *:80\nDocumentRoot /www/webstore/htdocs</textarea></td></tr>\n<tr><th>Buttons</th><td><input type=\"submit\" value=\"OK\"> <input type=\"submit\" value=\"Cancel\"> <input type=\"submit\" value=\"Apply\"> <input type=\"submit\" value=\"Refresh\"> <input type=\"submit\" value=\"Start\"> <input type=\"submit\" value=\"Stop\"> <input type=\"submit\" value=\"Restart\"> <input type=\"submit\" value=\"Delete\"> <input type=\"submit\" value=\"Add\"> <input type=\"submit\" value=\"Remove\"> <input type=\"submit\" value=\"Browse...\"> <input type=\"submit\" value=\"&lt; Back\"> <input type=\"submit\" value=\"Next &gt;\"> <input type=\"submit\" value=\"Finish\"></td></tr>\n<tr><th>Status words</th><td><span class=\"ok\">Running</span> <span class=\"ok\">Active</span> &nbsp; <span class=\"err\">Stopped</span> <span class=\"err\">Ended</span> <span class=\"err\">Error</span> &nbsp; (text colour only; everything else black)</td></tr>\n<tr><th>Utilisation bar</th><td><table class=\"bar\" cellspacing=\"0\" cellpadding=\"0\"><tr><td class=\"fill\" width=\"37%\"></td><td width=\"63%\"></td></tr></table> 37% &nbsp; <table class=\"bar\" cellspacing=\"0\" cellpadding=\"0\"><tr><td class=\"hot\" width=\"91%\"></td><td width=\"9%\"></td></tr></table> 91%</td></tr>\n<tr><th>Glyphs</th><td><span class=\"glyph\">[+]</span> collapsed &nbsp; <span class=\"glyph\">[-]</span> expanded &nbsp; (text glyphs; a 16&times;16 GIF may replace them)</td></tr>\n</table>\n";
    let out = "<pre class=\"out\">Job . . :   ADMIN          User . . :   QTMHHTTP     Number . . . . . . :   123456\n  MSGID      TYPE                    SEV  DATE      TIME             FROM PGM     LIBRARY\n  CPF1124    Information             00   10/29/26  06:00:01.117332  QWTPIIPP     QSYS\n             Message . . . . :   Job 123456/QTMHHTTP/ADMIN started on 10/29/26 at 06:00:01 in subsystem QHTTPSVR in QSYS.\n  HTP8001    Information             00   10/29/26  06:00:03.004210  QZHBMAIN     QHTTPSVR\n             Message . . . . :   HTTP server instance ADMIN started, listening on *:2001.</pre>";
    let body = format!(
        "<p class=\"small grey\">Every pattern the three products use, once. Use this page to check a new page against the standard before adding it.</p>\n\
<h2>1. Message area</h2>\n{}\n\
<h2>2. List pattern (filter row, sortable headers, per-row actions, paging)</h2>\n{}{}{}\n\
<p class=\"small grey\">Variant: a per-row <code>&lt;select&gt;</code> of actions plus \"Go\": <select name=\"act\"><option>--Select action--</option><option>Hold</option><option>Release</option><option>End</option><option>Job log</option><option>Properties</option></select> <input type=\"submit\" value=\"Go\"></p>\n\
<p class=\"small grey\">Empty state:</p>\n{}\n\
<h2>3. Properties pattern (tabs, grouped label/value rows, buttons)</h2>\n{}{}\n\
<h2>4. Wizard pattern</h2>\n{}\n\
<h2>5. Confirmation and second-level text</h2>\n<p class=\"small grey\">Confirmations are separate pages (see <a href=\"{}\">Confirm Delete Instance</a>). Second-level text is Message, Cause, Recovery, in that order (see <a href=\"{}\">Error page</a>).</p>\n\
<h2>6. Controls (browser defaults, never restyled)</h2>\n{}\n<h2>7. System output</h2>\n{out}\n",
        qsweb::partials::messages(
            &P,
            &[
                Message::new(Kind::Info, "CPF1321", "Job 123456/QUSER/QPADEV0001 ended."),
                Message::new(Kind::Done, "HTP8001", "HTTP server instance WEBSTORE started."),
                Message::new(Kind::Error, "SQL0204", "TABLE CUSTMAST in QGPL type *FILE not found."),
            ]
        ),
        filter(
            &[
                FilterField::Select(
                    "Subsystem".into(),
                    "sbs".into(),
                    ["*ALL", "QINTER", "QBATCH", "QHTTPSVR"].map(String::from).to_vec(),
                    String::new()
                ),
                FilterField::Text("User".into(), "user".into(), String::new(), 10),
                FilterField::Select(
                    "Status".into(),
                    "sts".into(),
                    ["*ALL", "RUN", "DEQW", "MSGW"].map(String::from).to_vec(),
                    String::new()
                ),
            ],
            false
        ),
        list_table(&jobs),
        paging(&Paging::from_query("1", "25", 288, Vec::new())),
        list_table(&empty),
        tabs(&[tab("General"), tab("Options"), tab("Security"), tab("Logs")], 0),
        props_body,
        wizard(
            &ctx,
            &["Name and description", "Server root", "Document root", "IP address and port", "Access log", "Create"],
            2,
            &wiz_body,
            &[Button::new("back", "< Back"), Button::new("next", "Next >"), Button::new("cancel", "Cancel")]
        ),
        PRODUCT.url("/instance/WEBSTORE/confirm?op=delete"),
        PRODUCT.url("/msg/CPF2204"),
        form(&ctx, "", controls),
    );
    let _ = buttons;
    let page = Page {
        title: "Pattern reference".into(),
        crumb: vec!["Shared".into(), "Pattern reference".into()],
        h1: "Pattern reference".into(),
        body,
        help: "components".into(),
        generic: true,
        ..Page::default()
    };
    // The prototype links the tabs back to itself and its error page
    // link to the page file; served, both are help windows.
    assert_golden(
        &render(&ctx, &page),
        "shared/components.html",
        &[
            ("<a href=\"components.html\">Options</a> | <a href=\"components.html\">Security</a> | <a href=\"components.html\">Logs</a>",
             "<a href=\"help.html\">Options</a> | <a href=\"help.html\">Security</a> | <a href=\"help.html\">Logs</a>"),
            ("<a href=\"error.html\">Error page</a>", "<a href=\"help.html\">Error page</a>"),
        ],
    );
}

#[test]
fn confirm_stop() {
    let mut b = Browser::signed_on();
    let r = b.get("/instance/INTRANET/confirm?op=stop");
    assert_eq!(r.status, 200);
    // The prototype leaves the How to end choice outside the form, so it
    // would never be submitted.
    assert_golden(
        &r.text(),
        "http/confirm-stop.html",
        &[
            ("</table><p class=\"small grey\">How to end:", "</table><form method=\"post\"><p class=\"small grey\">How to end:"),
            ("Immediate</p><form method=\"post\">", "Immediate</p>"),
        ],
    );
}

#[test]
fn confirm_delete() {
    let mut b = Browser::signed_on();
    let r = b.get("/instance/WEBSTORE/confirm?op=delete");
    assert_eq!(r.status, 200);
    assert_golden(&r.text(), "shared/confirm.html", &[]);
}

/// The wizard as the prototype shows it: creating `WEBSTORE` on a system
/// that has neither it nor `TESTSRV` yet.
#[test]
fn create_wizard() {
    let mut b = Browser::signed_on();
    for n in ["WEBSTORE", "TESTSRV"] {
        httpd::instance::delete(b.fx.files.as_ref(), n).unwrap();
    }
    let steps: [&[(&str, &str)]; 5] = [
        &[("name", "webstore"), ("desc", "Web store front end"), ("base", "new"), ("copyfrom", "APACHEDFT")],
        &[("root", "/www/webstore")],
        &[("docroot", "/www/webstore/htdocs"), ("sample", "on")],
        &[("addr", "*ALL"), ("ip", "10.0.10.20"), ("port", "80")],
        &[("log", "yes"), ("logpath", "/www/webstore/logs/access_log"), ("fmt", "Combined"), ("rot", "Daily")],
    ];
    assert_eq!(b.post("/create/step/1", &[("create_new_instance", "Create new instance...")]).status, 303);
    for (i, fields) in steps.iter().enumerate() {
        let n = i + 1;
        let mut f = fields.to_vec();
        f.push(("next", "Next >"));
        let r = b.post(&format!("/create/step/{n}"), &f);
        assert_eq!(r.header("Location").map(ToString::to_string), Some(PRODUCT.url(&format!("/create/step/{}", n + 1))));
        let r = b.get(&format!("/create/step/{n}"));
        assert_golden(&r.text(), &format!("http/create-{n}.html"), &[]);
    }
    let r = b.get("/create/step/6");
    assert_golden(&r.text(), "http/create-6.html", &[]);
}

#[test]
fn instance_general() {
    let mut b = Browser::signed_on();
    let r = b.get("/instance/WEBSTORE/general");
    assert_eq!(r.status, 200);
    assert_golden(&r.text(), "http/instance-general.html", &[]);
}
