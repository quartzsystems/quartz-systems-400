// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server: the pages against their approved renderings
//! (`tests/golden/nav/dirsrv-*.html`; `QS400_UPDATE_GOLDEN=1` rewrites
//! them after a deliberate change), and the changes the actions make
//! through the administration interface of the fixture.

mod common;

use common::{Browser, golden};
use dirsrv::admin::{DirAdmin, InstanceState, SchemaKind, SchemaQuery};
use navigator::env::Audit;
use qsweb::golden::{Normalize, diff};

/// The page at `route`, served with 200.
fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

/// Compare `served` with approved page `file`, or write it.
fn approved(b: &Browser, served: &str, file: &str) {
    let path = format!("{}/tests/golden/{file}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        let text = served.replace(&b.csrf, "CSRF");
        std::fs::write(&path, text).expect("writes the golden page");
        return;
    }
    let g = golden(file);
    if let Some(d) = diff(served, &g, &Normalize { url: &|_| None }) {
        panic!("{file} differs from the served page at {d}");
    }
}

const Q: &str = "/dirsrv/inst/QUSRDIR";

#[test]
fn pages_are_the_approved_ones() {
    let mut b = Browser::signed_on();
    for (route, file) in [
        ("/dirsrv".to_string(), "nav/dirsrv.html"),
        (
            "/dirsrv/inst/DEVDIR/confirm?op=start".into(),
            "nav/dirsrv-start.html",
        ),
        (format!("{Q}/confirm?op=end"), "nav/dirsrv-end.html"),
        (format!("{Q}/props"), "nav/dirsrv-props.html"),
        (format!("{Q}/admin"), "nav/dirsrv-admin.html"),
        (format!("{Q}/suffixes"), "nav/dirsrv-suffixes.html"),
        (
            format!("{Q}/suffixes/remove?dn=dc%3Dquartz%2Cdc%3Dexample"),
            "nav/dirsrv-suffix-remove.html",
        ),
        (format!("{Q}/indexes"), "nav/dirsrv-indexes.html"),
        (format!("{Q}/schema"), "nav/dirsrv-schema.html"),
        (
            format!("{Q}/schema?kind=Object+classes&q=person&show=Added"),
            "nav/dirsrv-schema-classes.html",
        ),
        (
            format!("{Q}/schema/attr/1.3.6.1.4.1.99999.1.1"),
            "nav/dirsrv-schema-def.html",
        ),
        (format!("{Q}/schema/add"), "nav/dirsrv-schema-add.html"),
        (
            format!("{Q}/access?dn=uid%3Djsmith%2Cou%3DPeople%2Cdc%3Dquartz%2Cdc%3Dexample"),
            "nav/dirsrv-access.html",
        ),
        (format!("{Q}/monitor"), "nav/dirsrv-monitor.html"),
        ("/dirsrv/publish".into(), "nav/dirsrv-publish.html"),
        (
            "/dirsrv/publish/Users".into(),
            "nav/dirsrv-publish-agent.html",
        ),
    ] {
        let t = ok(&mut b, &route);
        approved(&b, &t, file);
    }
    for key in [
        "dirsrv",
        "dsprops",
        "dsadmin",
        "dssuffixes",
        "dsindexes",
        "dsschema",
        "dsaccess",
        "dsmonitor",
        "dspublish",
    ] {
        assert!(
            navigator::help::TOPICS.iter().any(|t| t.key == key),
            "help {key}"
        );
    }
}

#[test]
fn instances_start_end_and_restart() {
    let mut b = Browser::signed_on();
    let r = b.post("/dirsrv/inst/DEVDIR/confirm?op=start", &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/dirsrv"));
    let l = b.fx.dirsrv.instances("QSECOFR").unwrap();
    assert_eq!(l[1].state, InstanceState::Running);
    assert!(ok(&mut b, "/dirsrv").contains("QDIRSRV/DEVDIR"));
    let r = b.post(&format!("{Q}/confirm?op=end"), &[("ok", "OK")]);
    assert_eq!(r.status, 303);
    assert_eq!(
        b.fx.dirsrv.instances("QSECOFR").unwrap()[0].state,
        InstanceState::Stopped
    );
    let t = ok(&mut b, &format!("{Q}/monitor"));
    assert!(t.contains("is not active"), "{t}");
    assert_eq!(b.get("/dirsrv/inst/NOSUCH/props").status, 404);
    assert_eq!(b.get(&format!("{Q}/confirm?op=explode")).status, 404);
}

fn props_fields(size: &str) -> Vec<(&str, &str)> {
    vec![
        ("apply", "Apply"),
        ("autostart", "*YES"),
        ("port", "389"),
        ("secure_port", "636"),
        ("security", "TLS"),
        ("allow_anon", "*NO"),
        ("idle_timeout", "300"),
        ("write_timeout", "120"),
        ("size_limit", size),
        ("time_limit", "900"),
        ("deref_aliases", "always"),
        ("paged_non_admin", "*YES"),
        ("paged_limit", "2"),
        ("sort_non_admin", "*YES"),
        ("sort_key_limit", "3"),
        ("persistent_search", "*YES"),
        ("max_persistent", "100"),
        ("read_only", "*NO"),
        ("schema_check", "V3_lenient"),
        ("pw_encryption", "ssha512"),
        ("pwd_policy", "*NO"),
        ("projected_read_only", "*YES"),
        ("changelog_max", "0"),
        ("referrals", ""),
        ("log_options", "m"),
        ("audit", "*NO"),
    ]
}

#[test]
fn properties_change_the_configuration_file() {
    let mut b = Browser::signed_on();
    let r = b.post(&format!("{Q}/props"), &props_fields("2000"));
    assert_eq!(
        r.header("Location"),
        Some("/qs400/navigator/dirsrv/inst/QUSRDIR/props")
    );
    let text = b.fx.dirsrv.conf_text("QUSRDIR").unwrap();
    assert!(text.contains("ibm-slapdSizeLimit: 2000"));
    assert!(text.contains("ibm-slapdSecurity: TLS"));
    assert!(text.contains("ibm-slapdAllowAnon: FALSE"));
    assert!(ok(&mut b, &format!("{Q}/props")).contains("GLD0123"));
    // A number that does not read stays on the form with GLD0232.
    let r = b.post(&format!("{Q}/props"), &props_fields("lots"));
    assert_eq!(r.status, 200);
    assert!(r.text().contains("GLD0232"), "{}", r.text());
    // The administrator's password, as CHGDIRSVRA DN(*ADMINDN pw).
    let r = b.post(
        &format!("{Q}/admin"),
        &[
            ("ok", "OK"),
            ("what", "*ADMINDN"),
            ("dn", ""),
            ("pw", "secret"),
            ("pw2", "secret"),
        ],
    );
    assert_eq!(r.status, 303);
    assert!(
        b.fx.dirsrv
            .admin("QSECOFR", "QUSRDIR")
            .unwrap()
            .password_set
    );
    let r = b.post(
        &format!("{Q}/admin"),
        &[
            ("ok", "OK"),
            ("what", "*UPDATEDN"),
            ("pw", "x"),
            ("pw2", "x"),
        ],
    );
    assert_eq!(r.status, 303);
    assert!(ok(&mut b, &format!("{Q}/admin")).contains("GLD0215"));
}

#[test]
fn suffixes_indexes_schema_and_access() {
    let mut b = Browser::signed_on();
    let r = b.post(
        &format!("{Q}/suffixes"),
        &[("add", "Add suffix"), ("dn", "o=branch")],
    );
    assert_eq!(r.status, 303);
    assert!(ok(&mut b, &format!("{Q}/suffixes")).contains("o=branch"));
    let r = b.post(
        &format!("{Q}/suffixes/remove?dn=o%3Dbranch"),
        &[("ok", "OK")],
    );
    assert_eq!(r.status, 303);
    assert!(!ok(&mut b, &format!("{Q}/suffixes")).contains("o=branch"));
    let r = b.post(
        &format!("{Q}/suffixes"),
        &[("add", "Add suffix"), ("dn", "not a dn")],
    );
    assert_eq!(r.status, 303);
    assert!(ok(&mut b, &format!("{Q}/suffixes")).contains("GLD0224"));

    let r = b.post(
        &format!("{Q}/indexes"),
        &[("add", "Add"), ("attr", "givenname"), ("e", "on")],
    );
    assert_eq!(r.status, 303);
    assert!(
        b.fx.dirsrv
            .indexes("QSECOFR", "QUSRDIR")
            .unwrap()
            .iter()
            .any(|i| i.attr == "givenName" && i.equality && !i.presence)
    );

    let r = b.post(
        &format!("{Q}/schema/add"),
        &[
            ("ok", "OK"),
            ("kind", "Attribute types"),
            (
                "text",
                "( 1.3.6.1.4.1.99999.1.2 NAME 'deskNumber' SYNTAX 1.3.6.1.4.1.1466.115.121.1.15 )",
            ),
        ],
    );
    assert_eq!(r.status, 303);
    let added =
        b.fx.dirsrv
            .schema(
                "QSECOFR",
                "QUSRDIR",
                &SchemaQuery {
                    custom_only: true,
                    kind: SchemaKind::Attribute,
                    ..SchemaQuery::default()
                },
            )
            .unwrap();
    assert_eq!(added.total, 2);
    let r = b.post(
        &format!("{Q}/schema/attr/1.3.6.1.4.1.99999.1.2/remove"),
        &[("ok", "OK")],
    );
    assert_eq!(r.status, 303);

    let dn = "uid%3Djsmith%2Cou%3DPeople%2Cdc%3Dquartz%2Cdc%3Dexample";
    let r = b.post(
        &format!("{Q}/access?dn={dn}"),
        &[
            ("ok", "OK"),
            ("ownacl", "*YES"),
            (
                "acl",
                "access-id:uid=jsmith,ou=People,dc=quartz,dc=example:normal:rwsc\r\ngroup:cn=anybody:normal:rsc",
            ),
            ("aclprop", "*YES"),
            ("ownowner", "*NO"),
            ("owners", ""),
            ("ownerprop", "*YES"),
        ],
    );
    assert_eq!(r.status, 303);
    let a =
        b.fx.dirsrv
            .access(
                "QSECOFR",
                "QUSRDIR",
                "uid=jsmith,ou=People,dc=quartz,dc=example",
            )
            .unwrap();
    assert!(a.explicit_acl && a.acl.len() == 2 && !a.explicit_owner);
    let r = b.post(
        &format!("{Q}/access?dn={dn}"),
        &[("ok", "OK"), ("ownacl", "*YES"), ("acl", "nonsense")],
    );
    assert_eq!(r.status, 303);
    assert!(ok(&mut b, &format!("{Q}/access?dn={dn}")).contains("CPF9898"));
}

#[test]
fn publishing_agents_change() {
    let mut b = Browser::signed_on();
    let r = b.post(
        "/dirsrv/publish/Printers",
        &[
            ("ok", "OK"),
            ("enabled", "*YES"),
            ("server", "ldap.quartz.example"),
            ("port", "636"),
            ("tls", "*YES"),
            ("bind", "cn=Publisher,dc=quartz,dc=example"),
            ("pwop", "*CHANGE"),
            ("pw", "pub"),
            ("parent", "ou=Printers,dc=quartz,dc=example"),
        ],
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/navigator/dirsrv/publish")
    );
    let p = b.fx.dirsrv.publishing("QSECOFR").unwrap();
    assert!(p[1].enabled && p[1].tls && p[1].password_set);
    assert!(ok(&mut b, "/dirsrv/publish").contains("ldap.quartz.example:636 (TLS)"));
}

#[test]
fn without_the_authority_nothing_changes() {
    let mut w = Browser::as_user("QPGMR");
    assert_eq!(w.get("/dirsrv").status, 403);
    let r = w.post(&format!("{Q}/props"), &props_fields("1"));
    assert_eq!(r.status, 403);
    let r = w.post("/dirsrv/inst/DEVDIR/confirm?op=start", &[("ok", "OK")]);
    assert_eq!(r.status, 403);
    assert_eq!(
        w.fx.dirsrv.instances("QSECOFR").unwrap()[1].state,
        InstanceState::Stopped
    );
    assert!(w.fx.system.audit.lock().unwrap().iter().any(
        |a| matches!(a, Audit::Refused { object, .. } if object.contains("Directory server"))
    ));
}
