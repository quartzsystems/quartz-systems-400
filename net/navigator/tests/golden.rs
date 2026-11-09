// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Served pages against the golden pages, with the fixture's data.
//! Golden corrections (`fix` arguments) are listed in docs/NAVIGATOR.md.

mod common;

use std::fmt::Write as _;

use common::{Browser, assert_golden};
use navigator::PRODUCT;
use qsweb::frame::{Ctx, NavGroup, Page, Product, render};
use qsweb::msg::{Kind, Message};
use qsweb::partials::{
    Button, Cell, Col, FilterField, List, Paging, Prop, PropGroup, buttons, filter, form,
    list_table, paging, props, props_table, tabs, wizard,
};

#[test]
fn signon() {
    let mut b = Browser::new();
    let r = b.get("/signon");
    assert_eq!(r.status, 200);
    // The form posts back to the sign-on route, not to the home page.
    assert_golden(
        &r.text(),
        "nav/signon.html",
        &[("action=\"home.html\"", "action=\"signon.html\"")],
    );
}

#[test]
fn home() {
    let mut b = Browser::signed_on();
    let r = b.get("/home?auto=60");
    assert_eq!(r.status, 200);
    // The hardware's model in place of the system's codes; main storage
    // is Memory, with what is in use and what holds it.
    let bar = "<table class=\"bar\" cellspacing=\"0\" cellpadding=\"0\"><tr><td class=\"fill\" width=\"33%\"></td><td width=\"67%\"></td></tr></table>";
    let memory =
        format!("Memory</td><td>21,904 MB in use of 65,536 MB &nbsp; 33.4% &nbsp; {bar}</td>");
    let mut used = String::from(
        "all active</a></td></tr></table>\n<h3>Memory use</h3><table class=\"status\" cellspacing=\"0\">",
    );
    for (l, v) in [
        ("Jobs", "9,377 MB &nbsp; <span class=\"grey\">14.3%</span>"),
        (
            "Storage service",
            "6,210 MB &nbsp; <span class=\"grey\">9.4%</span>",
        ),
        (
            "Kernel and file cache",
            "6,317 MB &nbsp; <span class=\"grey\">9.6%</span>",
        ),
    ] {
        let _ = write!(used, "<tr><td class=\"lbl\">{l}</td><td>{v}</td></tr>");
    }
    for (j, m) in [
        ("123411/QUSER/QZDASOINIT", "1,844"),
        ("123301/QSYS/QDBSRV01", "912"),
        ("123456/QTMHHTTP/ADMIN", "604"),
        ("123105/QSYS/QSPLMAINT", "233"),
        ("123488/TESTUSER/QPADEV0003", "118"),
    ] {
        let _ = write!(
            used,
            "<tr><td class=\"lbl\"><a href=\"job-props-general.html\">{j}</a></td><td>{m} MB</td></tr>"
        );
    }
    used.push_str("</table>");
    assert_golden(
        &r.text(),
        "nav/home.html",
        &[
            (
                "Model / type</td><td><span class=\"mono\">Q400-M2 / 9Q04</span>",
                "Model</td><td>Dell Inc. PowerEdge R650",
            ),
            ("Main storage</td><td>65,536 MB</td>", memory.as_str()),
            ("all active</a></td></tr></table>", used.as_str()),
        ],
    );
    // Off: no reload.
    assert!(!b.get("/home").text().contains("http-equiv=\"refresh\""));
}

/// The pattern reference, rebuilt from the partials alone.
#[test]
#[allow(clippy::too_many_lines)]
fn components() {
    static P: Product = Product {
        wordmark: "QS/400",
        crumb_root: "QS/400",
        prefix: "/qs400/navigator",
        nav: navigator::NAV,
        system_label: "System",
        nav_script: false,
    };
    let _: &[NavGroup] = P.nav;
    let ctx = Ctx {
        product: &P,
        system: "QSHQ400".into(),
        user: "QSECOFR".into(),
        csrf: "t".into(),
        open: None,
        here: String::new(),
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
        cols: ["Job name", "User", "Number", "Status"]
            .iter()
            .map(|c| Col::plain(c))
            .collect(),
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
                    Prop::new(
                        "Instance name",
                        "<span class=\"mono\">WEBSTORE</span>".into(),
                    ),
                    Prop::new(
                        "Description",
                        qsweb::partials::input("desc", "Web store front end", 40),
                    )
                    .help("desc"),
                ],
            },
            PropGroup {
                title: Some("Startup".into()),
                rows: vec![
                    Prop::new(
                        "Start with STRTCP",
                        qsweb::partials::checkbox("autostart", true),
                    ),
                    Prop::new(
                        "Server user profile",
                        qsweb::partials::input("usrprf", "QTMHHTTP", 10),
                    ),
                ],
            },
        ],
        &[
            Button::new("apply", "Apply"),
            Button::new("ok", "OK"),
            Button::new("cancel", "Cancel"),
        ],
    );
    let wiz_body = format!(
        "<h2>Server root</h2><p class=\"small grey\">The directory that holds the configuration, logs and content of the instance.</p>{}",
        props_table(
            &P,
            &[PropGroup {
                title: None,
                rows: vec![Prop::new(
                    "Server root",
                    qsweb::partials::input_cmd("root", "/www/webstore", 40)
                )],
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
                Message::new(
                    Kind::Done,
                    "HTP8001",
                    "HTTP server instance WEBSTORE started."
                ),
                Message::new(
                    Kind::Error,
                    "SQL0204",
                    "TABLE CUSTMAST in QGPL type *FILE not found."
                ),
            ]
        ),
        filter(
            &[
                FilterField::Select(
                    "Subsystem".into(),
                    "sbs".into(),
                    ["*ALL", "QINTER", "QBATCH", "QHTTPSVR"]
                        .map(String::from)
                        .to_vec(),
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
        tabs(
            &[tab("General"), tab("Options"), tab("Security"), tab("Logs")],
            0
        ),
        props_body,
        wizard(
            &ctx,
            &[
                "Name and description",
                "Server root",
                "Document root",
                "IP address and port",
                "Access log",
                "Create"
            ],
            2,
            &wiz_body,
            &[
                Button::new("back", "< Back"),
                Button::new("next", "Next >"),
                Button::new("cancel", "Cancel")
            ]
        ),
        PRODUCT.url("/sbs/QINTER/end"),
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
            (
                "<a href=\"components.html\">Options</a> | <a href=\"components.html\">Security</a> | <a href=\"components.html\">Logs</a>",
                "<a href=\"help.html\">Options</a> | <a href=\"help.html\">Security</a> | <a href=\"help.html\">Logs</a>",
            ),
            (
                "<a href=\"error.html\">Error page</a>",
                "<a href=\"help.html\">Error page</a>",
            ),
        ],
    );
}
