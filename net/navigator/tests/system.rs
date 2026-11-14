// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System, Network and Performance: the pages against the golden pages,
//! and what their actions do. Golden corrections are listed in
//! docs/NAVIGATOR.md.

mod common;

use common::{Browser, assert_golden};
use navigator::env::Audit;

/// A page's text, which must be served with 200.
fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

fn config_ran(b: &Browser) -> Vec<String> {
    b.fx.config.ran.lock().unwrap().clone()
}

fn net_ran(b: &Browser) -> Vec<String> {
    b.fx.net.ran.lock().unwrap().clone()
}

#[test]
fn system_values() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/sysval"), "nav/system-values.html", &[]);
    let t = ok(&mut b, "/sysval?cat=datetime");
    assert!(
        t.contains("<h2>Date and time</h2>") && t.contains("QTIMZON"),
        "{t}"
    );
    assert!(
        t.contains("href=\"?cat=datetime&amp;sort=n\""),
        "sort keeps the category"
    );
}

#[test]
fn security_values() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/sysval?cat=security"),
        "nav/security-values.html",
        &[],
    );
}

#[test]
fn sysval_change() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/sysval/QMAXSIGN"),
        "nav/sysval-change.html",
        &[],
    );
    // A value outside the allowed ones stays on the page, not run.
    let t = b
        .post("/sysval/QMAXSIGN", &[("v", "99"), ("ok", "OK")])
        .text();
    assert!(t.contains("CPD9B8D") && t.contains("value=\"99\""), "{t}");
    assert!(config_ran(&b).is_empty());
    // OK runs CHGSYSVAL and the list shows the new value.
    let r = b.post("/sysval/QMAXSIGN", &[("v", "5"), ("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/sysval"));
    assert_eq!(config_ran(&b), ["CHGSYSVAL SYSVAL(QMAXSIGN) VALUE('5')"]);
    let t = ok(&mut b, "/sysval");
    assert!(
        t.contains("CPI9C01") && t.contains("<td class=\"mono\">5</td>"),
        "{t}"
    );
    // Special values pass.
    b.post("/sysval/QMAXSIGN", &[("v", "*nomax"), ("ok", "OK")]);
    assert!(ok(&mut b, "/sysval").contains("*NOMAX"));
}

#[test]
fn security_value_needs_secadm() {
    let mut b = Browser::as_user("QSYSOPR");
    let r = b.post("/sysval/QMAXSIGN", &[("v", "4"), ("ok", "OK")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("CPF2207"));
    assert!(b.fx.config.ran.lock().unwrap().is_empty());
    assert!(
        b.fx.system
            .audit
            .lock()
            .unwrap()
            .iter()
            .any(|a| matches!(a, Audit::Refused { need, .. } if need == "*SECADM"))
    );
}

#[test]
fn disk_status() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/disk"), "nav/disk-status.html", &[]);
    let t = ok(&mut b, "/disk/9");
    assert!(t.contains("Y3C1B220"), "{t}");
    assert_eq!(b.get("/disk/99").status, 404);
}

#[test]
fn hardware_resources() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/hdwrsc"), "nav/hardware-resources.html", &[]);
    // A filter keeps the matching rows and their parents.
    let t = ok(&mut b, "/hdwrsc?t=Communications&s=Not+connected");
    assert!(
        t.contains("CMN04") && t.contains("LIN02") && t.contains("CEC01"),
        "{t}"
    );
    assert!(!t.contains("CMN03") && !t.contains("DD001"));
    // Vary off is confirmed, then VRYCFG runs.
    let t = ok(&mut b, "/hdwrsc/CMN04/confirm");
    assert!(
        t.contains("Confirm Vary Off")
            && t.contains("VRYCFG CFGOBJ(ETHLIN04) CFGTYPE(*LIN) STATUS(*OFF)"),
        "{t}"
    );
    let r = b.post("/hdwrsc/CMN04/confirm", &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/hdwrsc"));
    assert_eq!(
        config_ran(&b),
        ["VRYCFG CFGOBJ(ETHLIN04) CFGTYPE(*LIN) STATUS(*OFF)"]
    );
    assert!(ok(&mut b, "/hdwrsc").contains("CPC2606"));
    // A resource no description uses has nothing to vary.
    assert!(ok(&mut b, "/hdwrsc/DD005%20-%20DD012/varyon").contains("nothing to vary on"));
    // Nothing changes on a GET.
    assert_eq!(config_ran(&b).len(), 1);
}

#[test]
fn time_management() {
    let mut b = Browser::signed_on();
    // Sync now is ours, after the approved page's buttons.
    assert_golden(
        &ok(&mut b, "/time"),
        "nav/time-management.html",
        &[(
            "value=\"Cancel\"></div>",
            "value=\"Cancel\"><input type=\"submit\" name=\"sync\" value=\"Sync now\"></div>",
        )],
    );
    let r = b.post(
        "/time",
        &[
            ("tz", "QN0600CST3"),
            ("df", "MDY"),
            ("dsep", "/"),
            ("tsep", ":"),
            ("ta", "QS400/QTOTNTP (NTP client)"),
            ("ntp", "10.0.10.1, 10.0.10.2"),
            ("apply", "Apply"),
        ],
    );
    assert_eq!(r.header("Location"), Some("/qs400/navigator/time"));
    assert_eq!(
        config_ran(&b),
        [
            "CHGSYSVAL SYSVAL(QTIMZON) VALUE('QN0600CST3')",
            "CHGNTPA RMTSYS('10.0.10.1' '10.0.10.2')"
        ]
    );
    let t = ok(&mut b, "/time");
    assert!(
        t.contains("<option selected>QN0600CST3</option>") && t.contains("10.0.10.1, 10.0.10.2"),
        "{t}"
    );
    // Sync now asks the client for a poll and changes nothing else.
    assert!(t.contains("name=\"sync\" value=\"Sync now\""), "{t}");
    let r = b.post("/time", &[("ntp", "ignored"), ("sync", "Sync now")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/time"));
    assert_eq!(config_ran(&b).last().map(String::as_str), Some("SYNC NOW"));
}

#[test]
fn tcpip() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/tcp"),
        "nav/tcpip.html",
        &[("13 started", "14 started")],
    );
    let r = b.post(
        "/tcp",
        &[
            ("h", "qshq400"),
            ("d", "quartz.example"),
            ("ds", "quartz.example hq.quartz.example"),
            ("p", "*LOCAL"),
            ("n1", "10.0.10.1"),
            ("n2", "10.0.10.3"),
            ("n3", ""),
            ("f", "*YES"),
            ("apply", "Apply"),
        ],
    );
    assert_eq!(r.header("Location"), Some("/qs400/navigator/tcp"));
    assert_eq!(
        net_ran(&b),
        [
            "CHGTCPDMN INTNETADR('10.0.10.1' '10.0.10.3')",
            "CHGTCPA IPDTGFWD(*YES)"
        ]
    );
    assert!(ok(&mut b, "/tcp/hosts").contains("qshq400.quartz.example"));
}

#[test]
fn tcp_changes_need_iosyscfg() {
    let mut b = Browser::as_user("QSYSOPR");
    let r = b.post("/tcp/ifc/confirm?a=10.0.10.21&op=end", &[("ok", "OK")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("*IOSYSCFG"));
    assert!(b.fx.net.ran.lock().unwrap().is_empty());
}

#[test]
fn tcpip_interfaces() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/tcp/ifc"),
        "nav/tcpip-interfaces.html",
        &[(
            "<input type=\"submit\" value=\"Add...\"><input type=\"submit\" value=\"Refresh\">",
            "<input type=\"submit\" name=\"add\" value=\"Add...\"><input type=\"submit\" name=\"refresh\" value=\"Refresh\">",
        )],
    );
    // End is confirmed.
    let t = ok(&mut b, "/tcp/ifc/confirm?a=10.0.10.21&op=end");
    assert!(
        t.contains("Confirm End Interface")
            && t.contains("ENDTCPIFC INTNETADR(&#39;10.0.10.21&#39;)")
    );
    b.post("/tcp/ifc/confirm?a=10.0.10.21&op=end", &[("ok", "OK")]);
    // Start, Change and Add.
    b.post("/tcp/ifc/start?a=192.168.99.1", &[("ok", "OK")]);
    assert!(ok(&mut b, "/tcp/ifc?edit=10.0.10.20").contains("<h2>Change interface</h2>"));
    b.post(
        "/tcp/ifc",
        &[
            ("a", "10.0.10.20"),
            ("mask", "255.255.255.0"),
            ("mtu", "9000"),
            ("auto", "*YES"),
            ("alias", "MGMT"),
            ("chgifc", "Change"),
        ],
    );
    let r = b.post("/tcp/ifc", &[("add", "Add...")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/tcp/ifc?add=1"));
    b.post(
        "/tcp/ifc",
        &[
            ("a", "10.0.30.5"),
            ("line", "ethlin03"),
            ("mask", "255.255.255.0"),
            ("mtu", ""),
            ("auto", "*NO"),
            ("alias", ""),
            ("addifc", "Add"),
        ],
    );
    assert_eq!(
        net_ran(&b),
        [
            "ENDTCPIFC INTNETADR('10.0.10.21')",
            "STRTCPIFC INTNETADR('192.168.99.1')",
            "CHGTCPIFC INTNETADR('10.0.10.20') SUBNETMASK('255.255.255.0') MTU(9000) AUTOSTART(*YES) ALIASNAME(MGMT)",
            "ADDTCPIFC INTNETADR('10.0.30.5') LIND(ETHLIN03) SUBNETMASK('255.255.255.0') AUTOSTART(*NO)",
        ]
    );
    let t = ok(&mut b, "/tcp/ifc");
    assert!(
        t.contains("TCP1D03")
            && t.contains("10.0.30.5")
            && t.contains("<td class=\"num\">9000</td>"),
        "{t}"
    );
}

#[test]
fn tcpip_routes() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/tcp/rte"), "nav/tcpip-routes.html", &[]);
    let r = b.post("/tcp/rte", &[]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/tcp/rte?add=1"));
    let k = "?d=10.1.0.0&m=255.255.0.0&n=172.16.0.1";
    assert!(ok(&mut b, &format!("/tcp/rte{k}")).contains("<h2>Change route</h2>"));
    b.post(
        "/tcp/rte",
        &[
            ("d", "10.1.0.0"),
            ("m", "255.255.0.0"),
            ("n", "172.16.0.1"),
            ("metric", "3"),
            ("chgrte", "Change"),
        ],
    );
    b.post(&format!("/tcp/rte/confirm{k}"), &[("ok", "OK")]);
    assert_eq!(
        net_ran(&b),
        [
            "CHGTCPRTE RTEDEST('10.1.0.0') SUBNETMASK('255.255.0.0') NEXTHOP('172.16.0.1') METRIC(3)",
            "RMVTCPRTE RTEDEST('10.1.0.0') SUBNETMASK('255.255.0.0') NEXTHOP('172.16.0.1')",
        ]
    );
    assert!(!ok(&mut b, "/tcp/rte").contains("172.16.0.1"));
}

#[test]
fn servers() {
    let mut b = Browser::signed_on();
    let served = ok(&mut b, "/tcp/servers");
    assert_golden(
        &served,
        "nav/servers.html",
        &[(
            "<input type=\"submit\" value=\"Start all autostart servers\"><input type=\"submit\" value=\"Refresh\">",
            "<input type=\"submit\" name=\"start\" value=\"Start all autostart servers\"><input type=\"submit\" name=\"refresh\" value=\"Refresh\">",
        )],
    );
    assert!(
        served.contains("href=\"http://localhost:2001/qs400/httpadmin/instances\""),
        "{served}"
    );
    // End is confirmed; Start of an ended one; autostart.
    assert!(
        ok(&mut b, "/tcp/servers/TELNET/confirm?op=restart")
            .contains("STRTCPSVR SERVER(*TELNET) RESTART(*TELNET)")
    );
    b.post("/tcp/servers/TELNET/confirm?op=end", &[("ok", "OK")]);
    b.post("/tcp/servers/SNMP/start", &[("ok", "OK")]);
    b.post("/tcp/servers/SMTP", &[("auto", "*YES"), ("ok", "OK")]);
    b.post("/tcp/servers", &[("start", "Start all autostart servers")]);
    assert_eq!(
        net_ran(&b),
        [
            "ENDTCPSVR SERVER(*TELNET)",
            "STRTCPSVR SERVER(*SNMP)",
            "CHGSMTPA AUTOSTART(*YES)",
            "STRTCPSVR SERVER(*AUTOSTART)",
        ]
    );
    let t = ok(&mut b, "/tcp/servers");
    assert!(t.contains("TCP1A07"), "{t}");
}

#[test]
fn system_activity() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/perf/sys?a=10"),
        "nav/system-activity.html",
        &[],
    );
    let t = ok(&mut b, "/perf/sys");
    assert!(!t.contains("http-equiv=\"refresh\""));
    let t = ok(&mut b, "/perf/sys?sort=io");
    assert!(t.find("RGZPFM01").unwrap() < t.find("NIGHTLY").unwrap());
}

#[test]
fn disk_activity() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/perf/disk?a=10"),
        "nav/disk-activity.html",
        &[],
    );
}
