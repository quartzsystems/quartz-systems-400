// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `publish.conf` (docs/DIRSRV.md §10.1): the example, defaults, the
//! writer read back, the sealed password, and every refusal.

use certs::stash::KeySealer;
use dirsrv::publish::config::{
    self, AgentConfig, ConfigError, FORMAT_VERSION, Kind, PublishConfig, Security, aad,
    seal_password, unseal_password,
};

const EXAMPLE: &str = "version: 1

dn: cn=publishing
qs400-pubFormat: 1

dn: cn=users,cn=publishing
qs400-pubEnabled: TRUE
qs400-pubHost: ldap.example.com
qs400-pubPort: 389
qs400-pubSecurity: STARTTLS
qs400-pubBindDN: cn=Administrator
qs400-pubBindPW:: AAECAwQ=
qs400-pubParentDN: ou=Users,dc=example,dc=com
qs400-pubCreateParent: TRUE
qs400-pubRetry: 300
qs400-pubResync: 86400
";

fn head() -> String {
    "version: 1\n\ndn: cn=publishing\nqs400-pubFormat: 1\n".into()
}

fn with_users(lines: &str) -> String {
    format!("{}\ndn: cn=users,cn=publishing\n{lines}", head())
}

fn refused(src: &str) -> String {
    match config::parse(src.as_bytes()) {
        Ok(c) => panic!("accepted: {c:?}\n{src}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn the_example_reads() {
    let c = config::parse(EXAMPLE.as_bytes()).unwrap();
    let u = c.agent(Kind::Users);
    assert!(u.enabled);
    assert_eq!(u.host, "ldap.example.com");
    assert_eq!(u.port, 389);
    assert_eq!(u.security, Security::StartTls);
    assert_eq!(u.bind_dn.as_deref(), Some("cn=Administrator"));
    assert_eq!(u.bind_pw.as_deref(), Some(&[0u8, 1, 2, 3, 4][..]));
    assert_eq!(u.parent(), "ou=Users,dc=example,dc=com");
    assert!(u.create_parent);
    assert_eq!((u.retry, u.resync), (300, 86_400));
    assert_eq!(u.publisher_name("QSHQ400"), "cn=QS400 QSHQ400");
    // The printer agent is not in the file: shipped and disabled.
    assert_eq!(c.agent(Kind::Printers), &AgentConfig::new(Kind::Printers));
}

#[test]
fn only_the_header_is_the_shipped_state() {
    let c = config::parse(head().as_bytes()).unwrap();
    assert_eq!(c, PublishConfig::default());
    for k in Kind::ALL {
        let a = c.agent(k);
        assert!(!a.enabled);
        assert_eq!(a.host, "localhost");
        assert_eq!(a.port, 389);
        assert_eq!(a.parent(), k.default_parent());
    }
}

#[test]
fn the_writer_reads_back() {
    let mut c = PublishConfig::default();
    let s = KeySealer::for_tests();
    {
        let u = c.agent_mut(Kind::Users);
        u.enabled = true;
        u.host = "10.1.2.3".into();
        u.security = Security::Ssl;
        u.port = 6360;
        u.bind_dn = Some("cn=Manager, o=Example".into());
        u.bind_pw = Some(seal_password(&s, Kind::Users, "s3cret pw").unwrap());
        u.parent_dn = Some("ou=People,o=Example".into());
        u.create_parent = false;
        u.retry = 10;
        u.resync = 0;
        u.publisher = Some("QS400 TEST".into());
    }
    c.agent_mut(Kind::Printers).enabled = true;
    let text = config::write(&c);
    assert!(text.starts_with("version: 1\n"));
    assert!(text.contains(&format!("qs400-pubFormat: {FORMAT_VERSION}")));
    // The password is not in the file in the clear.
    assert!(!text.contains("s3cret"));
    let back = config::parse(text.as_bytes()).unwrap();
    assert_eq!(back, c);
    let pw = back.agent(Kind::Users).bind_pw.clone().unwrap();
    assert_eq!(unseal_password(&s, Kind::Users, &pw).unwrap(), "s3cret pw");
    // Written twice, the same.
    assert_eq!(config::write(&back), text);
}

#[test]
fn a_sealed_password_opens_only_for_its_agent() {
    let s = KeySealer::for_tests();
    let sealed = seal_password(&s, Kind::Users, "pw").unwrap();
    assert!(matches!(
        unseal_password(&s, Kind::Printers, &sealed),
        Err(ConfigError::Seal(_))
    ));
    let mut bent = sealed.clone();
    let last = bent.len() - 1;
    bent[last] ^= 1;
    assert!(unseal_password(&s, Kind::Users, &bent).is_err());
    assert_ne!(aad(Kind::Users), aad(Kind::Printers));
}

#[test]
fn defaults_follow_security_and_case_is_ignored() {
    let c = config::parse(
        format!(
            "{}\ndn: CN=Printers, CN=Publishing\nQS400-PUBSECURITY: ssl\nqs400-pubenabled: true\n",
            head()
        )
        .as_bytes(),
    )
    .unwrap();
    let p = c.agent(Kind::Printers);
    assert_eq!(p.security, Security::Ssl);
    assert_eq!(p.port, 636);
    assert!(p.enabled);
    // An explicit port stays.
    let c = config::parse(with_users("qs400-pubSecurity: SSL\nqs400-pubPort: 389\n").as_bytes())
        .unwrap();
    assert_eq!(c.agent(Kind::Users).port, 389);
}

#[test]
fn kinds_by_name_and_job() {
    assert_eq!(Kind::from_job("QGLDPUBE"), Some(Kind::Users));
    assert_eq!(Kind::from_job("qgldpuba"), Some(Kind::Printers));
    assert_eq!(Kind::from_job("QUSRDIR"), None);
    assert_eq!(Kind::from_name("Printers"), Some(Kind::Printers));
    assert_eq!(Kind::Users.key_attr(), "uid");
    assert_eq!(Kind::Printers.key_attr(), "printer-name");
}

#[test]
fn the_header_is_required() {
    assert!(refused("").contains("no cn=publishing"));
    assert!(
        refused("dn: cn=users,cn=publishing\nqs400-pubEnabled: TRUE\n").contains("first record")
    );
    assert!(refused("dn: cn=publishing\n").contains("missing"));
    assert!(refused("dn: cn=publishing\nqs400-pubFormat: 2\n").contains("format version 2"));
    assert!(refused("dn: cn=publishing\nqs400-pubFormat: one\n").contains("not a number"));
    assert!(config::parse(b"version: 2\n\ndn: cn=publishing\nqs400-pubFormat: 1\n").is_err());
    assert!(
        refused("dn: cn=publishing\nqs400-pubFormat: 1\nqs400-pubEnabled: TRUE\n")
            .contains("not a publishing attribute")
    );
}

#[test]
fn records_that_are_not_agents_are_refused() {
    let r = refused(&format!("{}\ndn: cn=groups,cn=publishing\n", head()));
    assert!(r.contains("not a publishing agent"), "{r}");
    let r = refused(&format!(
        "{}\ndn: cn=users,cn=publishing\n\ndn: cn=USERS,cn=publishing\n",
        head()
    ));
    assert!(r.contains("twice"), "{r}");
    let r = refused(&format!(
        "{}\ndn: cn=users,cn=publishing\nchangetype: delete\n",
        head()
    ));
    assert!(r.contains("change record"), "{r}");
}

#[test]
fn values_out_of_the_format_are_refused() {
    let cases: &[(&str, &str)] = &[
        ("objectClass: top\n", "not a publishing attribute"),
        (
            "qs400-pubEnabled: TRUE\nqs400-pubEnabled: FALSE\n",
            "more than once",
        ),
        ("qs400-pubEnabled: yes\n", "neither TRUE nor FALSE"),
        ("qs400-pubHost: two words\n", "not a host name"),
        ("qs400-pubHost: \n", "not a host name"),
        ("qs400-pubPort: 0\n", "1 to 65535"),
        ("qs400-pubPort: 65536\n", "1 to 65535"),
        ("qs400-pubPort: http\n", "not a number"),
        ("qs400-pubSecurity: TLS\n", "NONE, SSL or STARTTLS"),
        ("qs400-pubBindDN: not a dn\n", "not a DN"),
        ("qs400-pubBindPW:: AAE=\n", "without"),
        ("qs400-pubParentDN: ,,\n", "not a DN"),
        ("qs400-pubRetry: 5\n", "10 to 86400"),
        ("qs400-pubRetry: 86401\n", "10 to 86400"),
        ("qs400-pubResync: 30\n", "neither 0 nor 60"),
        ("qs400-pubPublisher: \n", "empty or longer"),
        ("qs400-pubHost:< file:///etc/hosts\n", "cannot be a URL"),
        ("qs400-pubHost:: //79\n", "not UTF-8"),
    ];
    for (lines, want) in cases {
        let r = refused(&with_users(lines));
        assert!(r.contains(want), "{lines:?}: {r}");
        assert!(r.starts_with("publish.conf line "), "{r}");
    }
    let long = format!("qs400-pubPublisher: {}\n", "P".repeat(129));
    assert!(refused(&with_users(&long)).contains("longer"));
}

#[test]
fn edge_values_are_taken() {
    let c = config::parse(
        with_users(
            "qs400-pubRetry: 10\nqs400-pubResync: 0\nqs400-pubPort: 65535\nqs400-pubPublisher: X\n",
        )
        .as_bytes(),
    )
    .unwrap();
    let u = c.agent(Kind::Users);
    assert_eq!((u.retry, u.resync, u.port), (10, 0, 65535));
    assert_eq!(u.publisher_name("ANY"), "X");
    let c = config::parse(with_users("qs400-pubRetry: 86400\nqs400-pubResync: 60\n").as_bytes())
        .unwrap();
    assert_eq!(c.agent(Kind::Users).resync, 60);
}

#[test]
fn not_ldif_is_refused() {
    assert!(matches!(
        config::parse(b"dn cn=publishing\n"),
        Err(ConfigError::Ldif(_))
    ));
}
