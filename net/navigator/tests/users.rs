// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Users and groups: the pages against the golden pages, and what their
//! actions do. Golden corrections are listed in docs/NAVIGATOR.md.

mod common;

use common::{Browser, assert_golden};

/// A page's text, which must be served with 200.
fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

fn ran(b: &Browser) -> Vec<String> {
    b.fx.users.ran.lock().unwrap().clone()
}

#[test]
fn users() {
    let mut b = Browser::signed_on();
    // The two buttons get names: Create user... and Refresh.
    assert_golden(
        &ok(&mut b, "/users"),
        "nav/users.html",
        &[(
            "<input type=\"submit\" value=\"Create user...\"><input type=\"submit\" value=\"Refresh\">",
            "<input type=\"submit\" name=\"create\" value=\"Create user...\"><input type=\"submit\" name=\"refresh\" value=\"Refresh\">",
        )],
    );
    // Filters: generic names, status, class, group (supplemental too).
    assert!(ok(&mut b, "/users?p=Q*").contains("6 entries"));
    assert!(ok(&mut b, "/users?s=*DISABLED").contains("2 entries"));
    assert!(ok(&mut b, "/users?c=*SYSOPR").contains("2 entries"));
    assert!(ok(&mut b, "/users?g=ACCTG").contains("5 entries"));
    let t = ok(&mut b, "/users?sort=p");
    assert!(t.find(">ASMITH<").unwrap() < t.find(">WEBUSER<").unwrap());
}

#[test]
#[allow(clippy::many_single_char_names)]
fn user_props() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/user/WEBUSER/general");
    assert_golden(
        &t,
        "nav/user-props.html",
        &[
            // Every group is offered.
            ("<option>OPS</option></select>", "<option>OPS</option><option>SERVICES</option><option>WAREHOUSE</option><option>ACCTG</option></select>"),
            // Reset password has its own name.
            ("<input type=\"submit\" value=\"Reset password...\">", "<input type=\"submit\" name=\"reset\" value=\"Reset password...\">"),
        ],
    );
    // Apply runs CHGUSRPRF with what changed.
    let f = form_of(&t);
    let mut fields: Vec<(&str, &str)> = f.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    for (k, v) in &mut fields {
        match *k {
            "t" => *v = "Web default user",
            "lc" => *v = "*PARTIAL",
            _ => {}
        }
    }
    fields.push(("a3", "on"));
    fields.push(("apply", "Apply"));
    let r = b.post("/user/WEBUSER/general", &fields);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/user/WEBUSER/general"));
    assert_eq!(
        ran(&b).last().unwrap(),
        "CHGUSRPRF USRPRF(WEBUSER) TEXT('Web default user') LMTCPB(*PARTIAL) SPCAUT(*JOBCTL)"
    );
    let t = ok(&mut b, "/user/WEBUSER/general");
    assert!(t.contains("CPC2205"));
    assert!(t.contains("value=\"Web default user\""));
    // Reset password.
    b.post("/user/WEBUSER/general", &[("pw", "Secret1"), ("exp", "on"), ("reset", "Reset password...")]);
    assert_eq!(ran(&b).last().unwrap(), "CHGUSRPRF USRPRF(WEBUSER) PASSWORD('Secret1') PWDEXP(*YES)");
    // Other tabs.
    assert!(ok(&mut b, "/user/WEBUSER/groups").contains(">WEBUSERS</a>"));
    assert!(ok(&mut b, "/user/ADMINS/groups").contains(">CWELLMAN</a>"));
    assert!(ok(&mut b, "/user/CWELLMAN/authorities").contains("INVDATA"));
    assert!(ok(&mut b, "/user/WEBUSER/personal").contains("name=\"lang\""));
    // Without *SECADM: Not authorized.
    let mut w = Browser::as_user("QPGMR");
    assert_eq!(w.post("/user/WEBUSER/general", &[("t", "x"), ("apply", "Apply")]).status, 403);
}

/// The fields a page's last POST form sends as served: text inputs,
/// selects' chosen options and checked boxes.
fn form_of(html: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let attr = |tag: &str, a: &str| {
        tag.split(&format!("{a}=\"")).nth(1).and_then(|r| r.split('"').next()).map(ToString::to_string)
    };
    for part in html.split('<').skip(1) {
        let tag = part.split('>').next().unwrap_or("");
        if tag.starts_with("input") && tag.contains("type=\"text\"") {
            out.push((attr(tag, "name").unwrap(), attr(tag, "value").unwrap_or_default()));
        } else if tag.starts_with("input") && tag.contains("type=\"checkbox\"") && tag.contains("checked") {
            out.push((attr(tag, "name").unwrap(), "on".into()));
        }
    }
    let mut rest = html;
    while let Some(i) = rest.find("<select name=\"") {
        let s = &rest[i..];
        let name = s["<select name=\"".len()..].split('"').next().unwrap().to_string();
        let end = s.find("</select>").unwrap();
        let body = &s[..end];
        let chosen = body
            .split("<option")
            .skip(1)
            .find(|o| o.starts_with(" selected"))
            .or_else(|| body.split("<option").nth(1))
            .map(|o| o.split('>').nth(1).unwrap_or("").split('<').next().unwrap_or("").to_string())
            .unwrap_or_default();
        if name != "rows" {
            out.push((name, chosen));
        }
        rest = &s[end..];
    }
    out
}

#[test]
fn delete_user_is_confirmed() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/user/JDOE/delete");
    assert!(t.contains("Confirm Delete User Profile") && t.contains("DLTUSRPRF USRPRF(JDOE) OWNOBJOPT(*NODLT)"));
    assert!(ran(&b).is_empty());
    // JDOE owns objects: *NODLT refuses.
    b.post("/user/JDOE/delete", &[("own", "*NODLT"), ("ok", "OK")]);
    assert!(ok(&mut b, "/users").contains("CPF2215"));
    b.post("/user/JDOE/delete", &[("own", "*CHGOWN"), ("to", "QSECOFR"), ("ok", "OK")]);
    assert_eq!(ran(&b).last().unwrap(), "DLTUSRPRF USRPRF(JDOE) OWNOBJOPT(*CHGOWN QSECOFR)");
    let t = ok(&mut b, "/users");
    assert!(t.contains("CPC2191") && t.contains("14 entries"));
    // A group with members cannot go.
    b.post("/user/ADMINS/delete", &[("own", "*NODLT"), ("ok", "OK")]);
    assert!(ok(&mut b, "/groups").contains("CPF2257"));
    // Reset password from the list.
    let t = ok(&mut b, "/user/ASMITH/password");
    assert!(t.contains("name=\"exp\" checked"));
    b.post("/user/ASMITH/password", &[("pw", "a"), ("pw2", "b"), ("exp", "on"), ("ok", "Reset password")]);
    assert!(ok(&mut b, "/user/ASMITH/password").contains("CPF2361"));
    b.post("/user/ASMITH/password", &[("pw", "Abc12"), ("pw2", "Abc12"), ("exp", "on"), ("ok", "Reset password")]);
    assert_eq!(ran(&b).last().unwrap(), "CHGUSRPRF USRPRF(ASMITH) PASSWORD('Abc12') PWDEXP(*YES)");
}

/// Walk the wizard to the summary with JSMITH's answers.
fn to_summary(b: &mut Browser) {
    let r = b.post("/users/create/step/1", &[("create", "Create user...")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/users/create/step/1"));
    b.post(
        "/users/create/step/1",
        &[("u", "jsmith"), ("t", "J. Smith - warehouse"), ("p1", "Pw1"), ("p2", "Pw1"), ("e", "on"), ("s", "*ENABLED"), ("next", "Next >")],
    );
    b.post("/users/create/step/2", &[("c", "*USER"), ("g", "WAREHOUSE"), ("sg", "*NONE"), ("next", "Next >")]);
    b.post(
        "/users/create/step/3",
        &[
            ("ip", "*NONE"),
            ("im", "MAIN"),
            ("cl", "WHSE"),
            ("lc", "*YES"),
            ("jd", "QGPL/QDFTJOBD"),
            ("oq", "*WRKSTN"),
            ("hd", "/home/JSMITH"),
            ("next", "Next >"),
        ],
    );
    let r = b.post("/users/create/step/4", &[("next", "Next >")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/users/create/step/5"));
}

#[test]
fn create_user_wizard() {
    let mut b = Browser::signed_on();
    let r = b.post("/users/create/step/1", &[("create", "Create user...")]);
    assert_eq!(r.status, 303);
    assert_golden(&ok(&mut b, "/users/create/step/1"), "nav/create-user-1.html", &[]);
    // A step not reached yet goes back to the furthest one.
    assert_eq!(b.get("/users/create/step/3").header("Location"), Some("/qs400/navigator/users/create/step/1"));
    to_summary(&mut b);
    assert_golden(&ok(&mut b, "/users/create/step/5"), "nav/create-user-5.html", &[]);
    // Back keeps the answers.
    b.post("/users/create/step/5", &[("md", "on"), ("mq", "on"), ("back", "< Back")]);
    b.post("/users/create/step/4", &[("back", "< Back")]);
    assert!(ok(&mut b, "/users/create/step/3").contains("value=\"WHSE\""));
    b.post("/users/create/step/3", &[("cl", "WHSE"), ("next", "Next >")]);
    b.post("/users/create/step/4", &[("next", "Next >")]);
    // Create runs the command, then the extras, and forgets the wizard.
    let r = b.post("/users/create/step/5", &[("md", "on"), ("mq", "on"), ("finish", "Create")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/users"));
    let ran = ran(&b);
    assert!(ran[0].starts_with("CRTUSRPRF USRPRF(JSMITH) PASSWORD('Pw1') PWDEXP(*YES)"), "{ran:?}");
    assert_eq!(&ran[1..], ["CRTDIR DIR('/home/JSMITH')", "CHGOWN OBJ('/home/JSMITH') NEWOWN(JSMITH)", "CRTMSGQ MSGQ(QUSRSYS/JSMITH)"]);
    let t = ok(&mut b, "/users");
    assert!(t.contains("CPC2213"), "the profile's message shows");
    assert!(t.contains(">JSMITH<") && t.contains("16 entries"));
    // Entered anew, the wizard is blank.
    b.post("/users/create/step/1", &[("create", "Create user...")]);
    assert!(ok(&mut b, "/users/create/step/1").contains("name=\"u\" value=\"\""));
    // Cancel forgets too.
    b.post("/users/create/step/1", &[("u", "TEMP02"), ("next", "Next >")]);
    b.post("/users/create/step/2", &[("cancel", "Cancel")]);
    b.get("/users/create/step/1");
    assert!(ok(&mut b, "/users/create/step/1").contains("name=\"u\" value=\"\""));
    // A name in use stays on step 1 with the message.
    b.post("/users/create/step/1", &[("u", "WEBUSER"), ("next", "Next >")]);
    assert!(ok(&mut b, "/users/create/step/1").contains("CPF2214"));
}

#[test]
fn groups() {
    let mut b = Browser::signed_on();
    b.get("/nav?close=Users%20and%20groups&back=/qs400/navigator/home");
    assert_golden(&ok(&mut b, "/groups"), "nav/groups.html", &[]);
    let r = b.post("/groups", &[]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/groups/create"));
    ok(&mut b, "/groups/create");
    b.post("/groups/create", &[("name", "pickers"), ("text", "Pickers"), ("ok", "Create")]);
    assert_eq!(ran(&b).last().unwrap(), "CRTUSRPRF USRPRF(PICKERS) PASSWORD(*NONE) GID(*GEN) TEXT('Pickers')");
    let t = ok(&mut b, "/groups");
    assert!(t.contains(">PICKERS<") && t.contains(">1032<"));
}

#[test]
fn authorization_lists() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/autl"), "nav/authorization-lists.html", &[]);
    b.get("/nav?close=Users%20and%20groups&back=/qs400/navigator/home");
    assert_golden(&ok(&mut b, "/autl/INVDATA"), "nav/autl-entries.html", &[]);
    // Add, change, remove.
    b.post("/autl/INVDATA", &[("u", "jdoe"), ("a", "*USE"), ("add", "Add")]);
    assert_eq!(ran(&b).last().unwrap(), "ADDAUTLE AUTL(INVDATA) USER(JDOE) AUT(*USE)");
    let t = ok(&mut b, "/autl/INVDATA?edit=JDOE");
    assert!(t.contains("Change entry"));
    b.post("/autl/INVDATA?edit=JDOE", &[("u", "JDOE"), ("a", "*CHANGE"), ("m", "on"), ("change", "Change")]);
    assert_eq!(ran(&b).last().unwrap(), "CHGAUTLE AUTL(INVDATA) USER(JDOE) AUT(*CHANGE *AUTLMGT)");
    let t = ok(&mut b, "/autl/INVDATA/remove?user=JDOE");
    assert!(t.contains("RMVAUTLE AUTL(INVDATA) USER(JDOE)"));
    b.post("/autl/INVDATA/remove?user=JDOE", &[("ok", "OK")]);
    assert!(!ok(&mut b, "/autl/INVDATA").contains(">JDOE<"));
    assert!(ok(&mut b, "/autl/INVDATA/objects").contains("HLINV"));
    // Create and delete.
    b.post("/autl/create", &[("name", "NEWLIST"), ("text", "New"), ("ok", "Create")]);
    assert!(ok(&mut b, "/autl").contains(">NEWLIST<"));
    assert!(ok(&mut b, "/autl/NEWLIST/delete").contains("DLTAUTL AUTL(NEWLIST)"));
    b.post("/autl/NEWLIST/delete", &[("ok", "OK")]);
    assert!(!ok(&mut b, "/autl").contains(">NEWLIST<"));
}
