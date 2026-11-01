// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Software: licensed programs, PTFs and fixes against the golden pages,
//! and the commands their actions run. Golden corrections are listed in
//! docs/NAVIGATOR.md.

mod common;

use common::{Browser, assert_golden};
use navigator::env::Audit;
use navigator::env::sw::PtfStatus;

/// The prototype's product IDs are not the system's (`QS00SS1`).
const IDS: (&str, &str) = ("5Q00", "QS00");

fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

fn ran(b: &Browser) -> Vec<String> {
    b.fx.sw.ran.lock().unwrap().clone()
}

fn named(value: &str, name: &str) -> (String, String) {
    (
        format!("<input type=\"submit\" value=\"{value}\">"),
        format!("<input type=\"submit\" name=\"{name}\" value=\"{value}\">"),
    )
}

fn fixes(list: &[(String, String)]) -> Vec<(&str, &str)> {
    list.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect()
}

#[test]
fn licensed_programs() {
    let mut b = Browser::signed_on();
    let names = [
        named("Install from image catalog...", "install"),
        named("Check compatibility", "check"),
        named("Display license keys", "keys"),
    ];
    let mut fx = fixes(&names);
    fx.push(IDS);
    assert_golden(&ok(&mut b, "/licpgm"), "nav/licensed-programs.html", &fx);
    let t = ok(&mut b, "/licpgm?s=Not%20installed");
    assert!(t.contains("QS00HA1") && !t.contains("QS00SS1"));
    assert!(ok(&mut b, "/licpgm/keys").contains("*NOMAX"));
    assert!(ok(&mut b, "/licpgm/QS00WDS/51").contains("ILE COBOL Compiler"));
    let r = b.post("/licpgm", &[("keys", "Display license keys")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/licpgm/keys"));
    b.post("/licpgm", &[("check", "Check compatibility")]);
    assert!(ok(&mut b, "/licpgm").contains("CPC0C20"));
}

#[test]
fn install_and_delete() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/licpgm/QS00HA1/*BASE/confirm?op=install&c=QS400V1R0");
    assert!(t.contains("RSTLICPGM LICPGM(QS00HA1) OPTION(*BASE) DEV(OPTVRT01)"), "{t}");
    b.post("/licpgm/QS00HA1/*BASE/confirm?op=install&c=QS400V1R0", &[("ok", "OK")]);
    let t = ok(&mut b, "/licpgm/QS00WDS/51/confirm?op=delete");
    assert!(t.contains("DLTLICPGM LICPGM(QS00WDS) OPTION(51)"));
    b.post("/licpgm/QS00WDS/51/confirm?op=delete", &[("cancel", "Cancel")]);
    b.post("/licpgm/QS00WDS/51/confirm?op=delete", &[("ok", "OK")]);
    assert_eq!(
        ran(&b),
        [
            "RSTLICPGM LICPGM(QS00HA1) OPTION(*BASE) DEV(OPTVRT01)",
            "DLTLICPGM LICPGM(QS00WDS) OPTION(51)"
        ]
    );
    assert!(ok(&mut b, "/licpgm").contains("CPC3D82"));
    // Changing software needs *ALLOBJ.
    let mut w = Browser::as_user("QSYSOPR");
    assert_eq!(w.post("/licpgm/QS00SS1/*BASE/confirm?op=delete", &[("ok", "OK")]).status, 403);
    assert!(w.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Refused { .. })));
    assert!(ran(&w).is_empty());
}

#[test]
fn ptfs() {
    let mut b = Browser::signed_on();
    let names = [
        named("Load from image catalog...", "load"),
        named("Apply all loaded at next IPL", "apply"),
        named("Display PTF level", "level"),
    ];
    let mut fx = fixes(&names);
    fx.push(IDS);
    assert_golden(&ok(&mut b, "/ptf"), "nav/ptfs.html", &fx);
    let t = ok(&mut b, "/ptf?s=Superseded&p=QS00NV1");
    assert!(t.contains("12 entries"), "{t}");
    assert!(ok(&mut b, "/ptf?id=QS10432").contains("1 entry"));
    assert!(ok(&mut b, "/ptf/QS10432/cover").contains("SAVACT lock timeout"));
    assert!(ok(&mut b, "/ptf/level").contains("QS400-CUM-26290"));
    // Check for fixes asks the update service.
    let r = b.get("/ptf?check=Check%20for%20fixes");
    assert_eq!(r.status, 303);
    assert_eq!(ran(&b), ["QSCHKUPD"]);
    assert!(ok(&mut b, "/ptf").contains("CPI3E01"));
}

#[test]
fn ptf_confirm() {
    let mut b = Browser::signed_on();
    let url = "/ptf/confirm?op=apply&id=QS10432&id=QS10447";
    assert_golden(
        &ok(&mut b, url),
        "nav/ptf-confirm.html",
        &[
            IDS,
            // The second PTF's description is the list's.
            ("<td>Security fix for TLS handshake</td>", "<td>Kernel: IPL time with large ASP</td>"),
            // The options sat outside the form, so OK never sent them.
            ("</table><p class=\"small grey\">Apply type:", "</table><form method=\"post\"><p class=\"small grey\">Apply type:"),
            ("</span></p><form method=\"post\"><div class=\"buttons\">", "</span></p><div class=\"buttons\">"),
        ],
    );
    b.post(url, &[("t", "*PERM"), ("d", "*NO"), ("ok", "OK")]);
    assert_eq!(ran(&b), ["APYPTF LICPGM(QS00SS1) SELECT(QS10432 QS10447) APY(*PERM) DELAYED(*YES)"]);
    let st = b.fx.sw.state.lock().unwrap().ptfs.iter().find(|p| p.id == "QS10447").unwrap().status;
    assert_eq!(st, PtfStatus::Permanent);
    assert!(ok(&mut b, "/ptf").contains("CPC3606"));
    // Remove, load, and the catalog.
    b.post("/ptf/confirm?op=remove&id=QS10426", &[("ok", "OK")]);
    b.post("/ptf/confirm?op=load&id=QS10462", &[("ok", "OK")]);
    b.post("/ptf/confirm?op=load&c=QS400PTF26290", &[("ok", "OK")]);
    assert_eq!(
        ran(&b)[1..],
        [
            "RMVPTF LICPGM(QS00DG1) SELECT(QS10426) RMV(*TEMP)",
            "LODPTF LICPGM(QS00SS1) DEV(*SAVF) SELECT(QS10462) SAVF(QGPL/QS10462)",
            "INSPTF LICPGM(*ALL) DEV(OPTVRT02) INSTYPE(*DLYALL)"
        ]
    );
    // The buttons lead to confirmations.
    let mut b = Browser::signed_on();
    let r = b.post("/ptf", &[("apply", "Apply all loaded at next IPL")]);
    let to = r.header("Location").unwrap_or_default().to_string();
    assert!(to.starts_with("/qs400/navigator/ptf/confirm?op=apply&id="), "{to}");
    // Apply is only by POST, and needs *ALLOBJ.
    let mut w = Browser::as_user("QSYSOPR");
    assert_eq!(w.post(url, &[("ok", "OK")]).status, 403);
}

#[test]
fn fixes_page() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/fixes"), "nav/fixes.html", &[IDS]);
    let t = ok(&mut b, "/ptf/confirm?op=order&id=QS10491");
    assert!(t.contains("SNDPTFORD PTFID((QS10491))"));
    b.post("/ptf/confirm?op=order&id=QS10491", &[("ok", "OK")]);
    b.post("/fixes", &[("s", "mirror.example.com"), ("a", "Daily"), ("p", ""), ("apply", "Apply")]);
    let r = b.post("/fixes", &[("s", "mirror.example.com"), ("a", "Daily"), ("p", ""), ("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/ptf"));
    assert_eq!(
        ran(&b),
        [
            "SNDPTFORD PTFID((QS10491))",
            "QSCHGUPDA URL('https://mirror.example.com/qs400') PROXY(*NONE) SCHEDULE(*DAILY)",
            "QSCHGUPDA URL('https://mirror.example.com/qs400') PROXY(*NONE) SCHEDULE(*DAILY)"
        ]
    );
    let t = ok(&mut b, "/fixes");
    assert!(t.contains("value=\"mirror.example.com\"") && t.contains("<option selected>Daily</option>"));
}
