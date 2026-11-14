// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writes, binds, extended operations, controls, referrals, aliases,
//! the change log and persistent search against a directory in memory.

#![allow(clippy::many_single_char_names, clippy::similar_names, clippy::too_many_lines)]

mod harness;

use harness::{T, dns, done, names, text, vals};
use ldap::ResultCode;
use ldap::control::{
    Control, ControlValue, EntryChange, PasswordPolicyResponse, PersistentSearch, PolicyError,
    PostReadRequest, PostReadResponse, PreReadRequest, PreReadResponse, TreeDelete, change_type,
};
use ldap::ext::{self, PasswordModifyRequest, PasswordModifyResponse};
use ldap::msg::{Attribute, BindAuth, ModOp, Modification, ProtocolOp, Scope};

fn m(op: ModOp, ty: &str, vals: &[&str]) -> Modification {
    Modification {
        op,
        attr: Attribute::new(ty, vals),
    }
}

fn modify(t: &mut T, s: &dirsrv::ops::Session, dn: &str, mods: Vec<Modification>) -> ResultCode {
    let o = t.run(
        s,
        ProtocolOp::ModifyRequest {
            dn: dn.into(),
            changes: mods,
        },
        Vec::new(),
    );
    T::code(&o)
}

fn delete(t: &mut T, s: &dirsrv::ops::Session, dn: &str, ctls: Vec<Control>) -> ResultCode {
    let o = t.run(s, ProtocolOp::DelRequest(dn.into()), ctls);
    T::code(&o)
}

fn moddn(
    t: &mut T,
    s: &dirsrv::ops::Session,
    dn: &str,
    rdn: &str,
    del: bool,
    sup: Option<&str>,
) -> ResultCode {
    let o = t.run(
        s,
        ProtocolOp::ModDnRequest {
            dn: dn.into(),
            new_rdn: rdn.into(),
            delete_old_rdn: del,
            new_superior: sup.map(String::from),
        },
        Vec::new(),
    );
    T::code(&o)
}

fn ou(t: &mut T, name: &str, parent: &str) {
    let a = t.admin.clone();
    t.add_ok(
        &a,
        &format!("ou={name},{parent}"),
        &[
            text("objectClass", &["organizationalUnit"]),
            text("ou", &[name]),
        ],
    );
}

#[test]
fn add_stores_what_the_server_keeps() {
    let mut t = T::new();
    let a = t.admin.clone();
    t.person("joe", "secret1");
    let r = t.search(
        &a,
        "cn=joe,o=Acme",
        Scope::Base,
        "(objectClass=*)",
        &["*", "+"],
    );
    // Superclasses implied and stored; the structural class recorded.
    let mut oc = vals(&r, "objectClass");
    oc.sort();
    assert_eq!(
        oc,
        ["inetOrgPerson", "organizationalPerson", "person", "top"]
    );
    assert_eq!(vals(&r, "structuralObjectClass"), ["inetOrgPerson"]);
    assert_eq!(vals(&r, "creatorsName"), ["cn=Administrator"]);
    assert_eq!(vals(&r, "createTimestamp"), ["20261113120000.000000Z"]);
    assert_eq!(vals(&r, "entryUUID"), vals(&r, "ibm-entryUuid"));
    assert_eq!(vals(&r, "entryUUID")[0].len(), 36);
    let pw = vals(&r, "userPassword");
    assert!(pw[0].starts_with("{SSHA512}"), "{pw:?}");
    // Failures.
    let o = t.add(
        &a,
        "cn=joe,o=Acme",
        &[
            text("objectClass", &["person"]),
            text("cn", &["joe"]),
            text("sn", &["x"]),
        ],
    );
    assert_eq!(T::code(&o), ResultCode::EntryAlreadyExists);
    let o = t.add(
        &a,
        "cn=x,ou=none,o=Acme",
        &[
            text("objectClass", &["person"]),
            text("cn", &["x"]),
            text("sn", &["x"]),
        ],
    );
    assert_eq!(T::code(&o), ResultCode::NoSuchObject);
    assert_eq!(o.replies[0].op.result().unwrap().matched_dn, "O=ACME");
    let o = t.add(
        &a,
        "cn=y,o=Acme",
        &[text("objectClass", &["person"]), text("cn", &["y"])],
    );
    assert_eq!(T::code(&o), ResultCode::ObjectClassViolation);
    let o = t.add(
        &a,
        "cn=y,o=Acme",
        &[
            text("objectClass", &["person"]),
            text("cn", &["y"]),
            text("sn", &["y"]),
            text("createTimestamp", &["20200101000000Z"]),
        ],
    );
    assert_eq!(T::code(&o), ResultCode::ConstraintViolation);
    let o = t.add(
        &a,
        "cn=y,o=Acme",
        &[
            text("objectClass", &["person"]),
            text("cn", &["z"]),
            text("sn", &["y"]),
        ],
    );
    assert_eq!(T::code(&o), ResultCode::NamingViolation);
    let o = t.add(
        &a,
        "cn=y,o=Acme",
        &[
            text("objectClass", &["person"]),
            text("cn", &["y"]),
            text("sn", &["y"]),
            text("ibm-filterAclEntry", &["x"]),
        ],
    );
    assert_eq!(T::code(&o), ResultCode::UnwillingToPerform);
    let o = t.add(
        &a,
        "cn=y,o=Acme",
        &[
            text("objectClass", &["person"]),
            text("cn", &["y"]),
            text("sn", &["y"]),
            text("aclEntry", &["access-id:cn=a:object:r"]),
        ],
    );
    assert_eq!(T::code(&o), ResultCode::InvalidAttributeSyntax);
    let o = t.add(
        &a,
        "cn=y",
        &[
            text("objectClass", &["person"]),
            text("cn", &["y"]),
            text("sn", &["y"]),
        ],
    );
    assert_eq!(T::code(&o), ResultCode::NoSuchObject);
    // Anonymous may not add.
    let anon = dirsrv::ops::Session::anonymous();
    let o = t.add(
        &anon,
        "cn=y,o=Acme",
        &[
            text("objectClass", &["person"]),
            text("cn", &["y"]),
            text("sn", &["y"]),
        ],
    );
    assert_eq!(T::code(&o), ResultCode::InsufficientAccessRights);
}

#[test]
fn modify_operations() {
    let mut t = T::new();
    let a = t.admin.clone();
    t.person("joe", "pw");
    let dn = "cn=joe,o=Acme";
    assert_eq!(
        modify(
            &mut t,
            &a,
            dn,
            vec![m(ModOp::Add, "mail", &["joe@acme.test"])]
        ),
        ResultCode::Success
    );
    assert_eq!(
        modify(
            &mut t,
            &a,
            dn,
            vec![m(ModOp::Add, "mail", &["JOE@acme.test"])]
        ),
        ResultCode::AttributeOrValueExists
    );
    assert_eq!(
        modify(
            &mut t,
            &a,
            dn,
            vec![m(ModOp::Delete, "mail", &["nobody@x"])]
        ),
        ResultCode::NoSuchAttribute
    );
    assert_eq!(
        modify(&mut t, &a, dn, vec![m(ModOp::Delete, "description", &[])]),
        ResultCode::NoSuchAttribute
    );
    assert_eq!(
        modify(
            &mut t,
            &a,
            dn,
            vec![m(ModOp::Replace, "description", &["a", "b"])]
        ),
        ResultCode::Success
    );
    assert_eq!(
        modify(&mut t, &a, dn, vec![m(ModOp::Replace, "description", &[])]),
        ResultCode::Success
    );
    assert_eq!(
        modify(&mut t, &a, dn, vec![m(ModOp::Delete, "cn", &["joe"])]),
        ResultCode::NotAllowedOnRdn
    );
    assert_eq!(
        modify(&mut t, &a, dn, vec![m(ModOp::Delete, "sn", &[])]),
        ResultCode::ObjectClassViolation
    );
    assert_eq!(
        modify(&mut t, &a, dn, vec![m(ModOp::Replace, "entryUUID", &["x"])]),
        ResultCode::ConstraintViolation
    );
    assert_eq!(
        modify(&mut t, &a, dn, vec![m(ModOp::Add, "nosuch", &["x"])]),
        ResultCode::UndefinedAttributeType
    );
    // Increment (RFC 4525).
    assert_eq!(
        modify(
            &mut t,
            &a,
            dn,
            vec![m(ModOp::Increment, "employeeNumber", &["1"])]
        ),
        ResultCode::NoSuchAttribute
    );
    let a2 = a.clone();
    t.add_ok(
        &a2,
        "uid=p,o=Acme",
        &[
            text("objectClass", &["account", "posixAccount"]),
            text("uid", &["p"]),
            text("cn", &["p"]),
            text("uidNumber", &["10"]),
            text("gidNumber", &["1"]),
            text("homeDirectory", &["/home/p"]),
        ],
    );
    assert_eq!(
        modify(
            &mut t,
            &a,
            "uid=p,o=Acme",
            vec![m(ModOp::Increment, "uidNumber", &["5"])]
        ),
        ResultCode::Success
    );
    let r = t.search(
        &a,
        "uid=p,o=Acme",
        Scope::Base,
        "(objectClass=*)",
        &["uidNumber"],
    );
    assert_eq!(vals(&r, "uidNumber"), ["15"]);
    assert_eq!(
        modify(&mut t, &a, dn, vec![m(ModOp::Increment, "cn", &["1"])]),
        ResultCode::ConstraintViolation
    );
    // All or nothing: the second change fails, the first is not kept.
    assert_eq!(
        modify(
            &mut t,
            &a,
            dn,
            vec![
                m(ModOp::Add, "title", &["boss"]),
                m(ModOp::Add, "nosuch", &["x"])
            ]
        ),
        ResultCode::UndefinedAttributeType
    );
    let r = t.search(
        &a,
        dn,
        Scope::Base,
        "(objectClass=*)",
        &["title", "modifiersName"],
    );
    assert!(vals(&r, "title").is_empty());
    // The schema check after the change; structural class changes refused
    // only by V3.
    assert_eq!(
        modify(
            &mut t,
            &a,
            dn,
            vec![m(ModOp::Add, "objectClass", &["nosuchclass"])]
        ),
        ResultCode::ObjectClassViolation
    );
}

#[test]
fn delete_and_tree_delete() {
    let mut t = T::new();
    let a = t.admin.clone();
    ou(&mut t, "Sales", "o=Acme");
    ou(&mut t, "East", "ou=Sales,o=Acme");
    ou(&mut t, "West", "ou=Sales,o=Acme");
    assert_eq!(
        delete(&mut t, &a, "ou=Sales,o=Acme", Vec::new()),
        ResultCode::NotAllowedOnNonLeaf
    );
    let anon = dirsrv::ops::Session::anonymous();
    assert_eq!(
        delete(&mut t, &anon, "ou=East,ou=Sales,o=Acme", Vec::new()),
        ResultCode::InsufficientAccessRights
    );
    let o = t.run(
        &a,
        ProtocolOp::DelRequest("ou=West,ou=Sales,o=Acme".into()),
        vec![PreReadRequest(vec!["ou".into()]).to_control(false)],
    );
    assert_eq!(T::code(&o), ResultCode::Success);
    let pre = PreReadResponse::find(&o.replies[0].controls)
        .unwrap()
        .unwrap();
    assert_eq!(pre.0.attrs[0].vals, [b"West".to_vec()]);
    assert_eq!(
        delete(
            &mut t,
            &a,
            "ou=Sales,o=Acme",
            vec![TreeDelete.to_control(true)]
        ),
        ResultCode::Success
    );
    let r = t.search(&a, "o=Acme", Scope::Sub, "(objectClass=*)", &["1.1"]);
    assert_eq!(dns(&r), ["o=Acme"]);
    assert_eq!(
        delete(&mut t, &a, "ou=gone,o=Acme", Vec::new()),
        ResultCode::NoSuchObject
    );
    // Tree delete is a delete control only.
    let o = t.run(
        &a,
        ProtocolOp::ModifyRequest {
            dn: "o=Acme".into(),
            changes: vec![m(ModOp::Add, "description", &["x"])],
        },
        vec![TreeDelete.to_control(true)],
    );
    assert_eq!(T::code(&o), ResultCode::UnavailableCriticalExtension);
}

#[test]
fn modify_dn_and_subtree_moves() {
    let mut t = T::new();
    let a = t.admin.clone();
    ou(&mut t, "A", "o=Acme");
    ou(&mut t, "B", "o=Acme");
    t.add_ok(
        &a,
        "cn=joe,ou=A,o=Acme",
        &[
            text("objectClass", &["person"]),
            text("cn", &["joe"]),
            text("sn", &["s"]),
        ],
    );
    assert_eq!(
        moddn(&mut t, &a, "cn=joe,ou=A,o=Acme", "cn=joseph", false, None),
        ResultCode::Success
    );
    let r = t.search(
        &a,
        "cn=joseph,ou=A,o=Acme",
        Scope::Base,
        "(objectClass=*)",
        &["cn"],
    );
    let mut cn = vals(&r, "cn");
    cn.sort();
    assert_eq!(cn, ["joe", "joseph"]);
    assert_eq!(
        moddn(&mut t, &a, "cn=joseph,ou=A,o=Acme", "cn=jo", true, None),
        ResultCode::Success
    );
    let r = t.search(
        &a,
        "cn=jo,ou=A,o=Acme",
        Scope::Base,
        "(objectClass=*)",
        &["cn"],
    );
    let mut cn = vals(&r, "cn");
    cn.sort();
    assert_eq!(cn, ["jo", "joe"]);
    // Move the whole of ou=A under ou=B.
    assert_eq!(
        moddn(
            &mut t,
            &a,
            "ou=A,o=Acme",
            "ou=A",
            false,
            Some("ou=B,o=Acme")
        ),
        ResultCode::Success
    );
    let r = t.search(&a, "ou=B,o=Acme", Scope::Sub, "(cn=jo)", &["1.1"]);
    assert_eq!(dns(&r), ["cn=jo,ou=A,ou=B,o=Acme"]);
    assert_eq!(
        moddn(
            &mut t,
            &a,
            "ou=B,o=Acme",
            "ou=B",
            false,
            Some("ou=A,ou=B,o=Acme")
        ),
        ResultCode::UnwillingToPerform
    );
    assert_eq!(
        moddn(
            &mut t,
            &a,
            "ou=A,ou=B,o=Acme",
            "ou=A",
            false,
            Some("ou=none,o=Acme")
        ),
        ResultCode::NoSuchObject
    );
    ou(&mut t, "C", "o=Acme");
    assert_eq!(
        moddn(&mut t, &a, "ou=C,o=Acme", "ou=B", false, None),
        ResultCode::EntryAlreadyExists
    );
    assert_eq!(
        moddn(&mut t, &a, "o=Acme", "o=Other", false, None),
        ResultCode::UnwillingToPerform
    );
    assert_eq!(
        moddn(&mut t, &a, "ou=C,o=Acme", "bad rdn", false, None),
        ResultCode::InvalidDnSyntax
    );
    let anon = dirsrv::ops::Session::anonymous();
    assert_eq!(
        moddn(&mut t, &anon, "ou=C,o=Acme", "ou=D", true, None),
        ResultCode::InsufficientAccessRights
    );
}

#[test]
fn read_controls() {
    let mut t = T::new();
    let a = t.admin.clone();
    t.person("ann", "x");
    let o = t.run(
        &a,
        ProtocolOp::ModifyRequest {
            dn: "cn=ann,o=Acme".into(),
            changes: vec![m(ModOp::Replace, "sn", &["New"])],
        },
        vec![
            PreReadRequest(vec!["sn".into()]).to_control(true),
            PostReadRequest(vec!["sn".into()]).to_control(true),
        ],
    );
    assert_eq!(T::code(&o), ResultCode::Success);
    let pre = PreReadResponse::find(&o.replies[0].controls)
        .unwrap()
        .unwrap();
    let post = PostReadResponse::find(&o.replies[0].controls)
        .unwrap()
        .unwrap();
    assert_eq!(pre.0.attrs[0].vals, [b"Sann".to_vec()]);
    assert_eq!(post.0.attrs[0].vals, [b"New".to_vec()]);
    let o = t.run(
        &a,
        ProtocolOp::AddRequest {
            dn: "ou=X,o=Acme".into(),
            attrs: vec![
                text("objectClass", &["organizationalUnit"]),
                text("ou", &["X"]),
            ],
        },
        vec![PostReadRequest(vec!["entryUUID".into()]).to_control(true)],
    );
    let post = PostReadResponse::find(&o.replies[0].controls)
        .unwrap()
        .unwrap();
    assert_eq!(post.0.attrs[0].ty, "entryUUID");
    // The assertion control on a write.
    let o = t.run(
        &a,
        ProtocolOp::ModifyRequest {
            dn: "cn=ann,o=Acme".into(),
            changes: vec![m(ModOp::Replace, "sn", &["Newer"])],
        },
        vec![ldap::control::Assertion(ldap::Filter::parse("(sn=Old)").unwrap()).to_control(true)],
    );
    assert_eq!(T::code(&o), ResultCode::Other(122));
}

#[test]
fn referrals_and_manage_dsa_it() {
    let mut t = T::new();
    let a = t.admin.clone();
    t.add_ok(
        &a,
        "ou=Far,o=Acme",
        &[
            text("objectClass", &["referral", "extensibleObject"]),
            text("ou", &["Far"]),
            text("ref", &["ldap://far.test/ou=Far,o=Far"]),
        ],
    );
    let r = t.search(
        &a,
        "cn=x,ou=Far,o=Acme",
        Scope::Base,
        "(objectClass=*)",
        &[],
    );
    assert_eq!(done(&r).code, ResultCode::Referral);
    assert_eq!(done(&r).referral, ["ldap://far.test/cn=x,ou=Far,o=Far"]);
    let r = t.search(&a, "o=Acme", Scope::Sub, "(objectClass=*)", &["1.1"]);
    assert!(r.iter().any(|x| matches!(&x.op, ProtocolOp::SearchResultReference(u) if u == &["ldap://far.test/ou=Far,o=Far".to_string()])));
    let r = t.search_req(
        &a,
        T::req("ou=Far,o=Acme", Scope::Base, "(objectClass=*)", &["ref"]),
        vec![ldap::control::ManageDsaIt.to_control(true)],
    );
    assert_eq!(vals(&r, "ref"), ["ldap://far.test/ou=Far,o=Far"]);
    // The default referral answers outside every suffix.
    let mut s = harness::settings();
    s.referral = vec!["ldap://up.test".into()];
    let mut t = T::with(s);
    let anon = dirsrv::ops::Session::anonymous();
    let r = t.search(&anon, "dc=elsewhere", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::Referral);
    assert_eq!(done(&r).referral, ["ldap://up.test"]);
}

#[test]
fn aliases() {
    let mut t = T::new();
    let a = t.admin.clone();
    ou(&mut t, "People", "o=Acme");
    ou(&mut t, "Links", "o=Acme");
    t.add_ok(
        &a,
        "cn=joe,ou=People,o=Acme",
        &[
            text("objectClass", &["person"]),
            text("cn", &["joe"]),
            text("sn", &["s"]),
        ],
    );
    t.add_ok(
        &a,
        "cn=j,ou=Links,o=Acme",
        &[
            text("objectClass", &["alias", "extensibleObject"]),
            text("cn", &["j"]),
            text("aliasedObjectName", &["cn=joe,ou=People,o=Acme"]),
        ],
    );
    let r = t.search(
        &a,
        "cn=j,ou=Links,o=Acme",
        Scope::Base,
        "(objectClass=*)",
        &["1.1"],
    );
    assert_eq!(dns(&r), ["cn=joe,ou=People,o=Acme"]);
    let r = t.search(
        &a,
        "ou=Links,o=Acme",
        Scope::One,
        "(objectClass=person)",
        &["1.1"],
    );
    assert_eq!(dns(&r), ["cn=joe,ou=People,o=Acme"]);
    let mut req = T::req(
        "cn=j,ou=Links,o=Acme",
        Scope::Base,
        "(objectClass=*)",
        &["1.1"],
    );
    req.deref = ldap::msg::DerefAliases::Never;
    let r = t.search_req(&a, req, Vec::new());
    assert_eq!(dns(&r), ["cn=j,ou=Links,o=Acme"]);
    t.add_ok(
        &a,
        "cn=dead,ou=Links,o=Acme",
        &[
            text("objectClass", &["alias", "extensibleObject"]),
            text("cn", &["dead"]),
            text("aliasedObjectName", &["cn=nobody,o=Acme"]),
        ],
    );
    let r = t.search(
        &a,
        "cn=dead,ou=Links,o=Acme",
        Scope::Base,
        "(objectClass=*)",
        &[],
    );
    assert_eq!(done(&r).code, ResultCode::AliasProblem);
}

fn bind_ext(t: &mut T, mech: &str, conn: &dirsrv::ops::Conn) -> (ResultCode, dirsrv::ops::Session) {
    let msg = T::msg(
        ProtocolOp::BindRequest {
            version: 3,
            name: String::new(),
            auth: BindAuth::Sasl {
                mechanism: mech.into(),
                credentials: None,
            },
        },
        Vec::new(),
    );
    let o = t.bind_msg(&msg, conn);
    (T::code(&o), o.session.unwrap())
}

#[test]
fn binds() {
    let mut t = T::new();
    t.person("joe", "pw1");
    let (s, r, _) = t.bind("cn=Administrator", "secret");
    assert_eq!(r.code, ResultCode::Success);
    assert!(s.admin);
    assert_eq!(
        t.bind("CN=ADMINISTRATOR", "wrong").1.code,
        ResultCode::InvalidCredentials
    );
    let (s, r, _) = t.bind("cn=JOE, o=acme", "pw1");
    assert_eq!(r.code, ResultCode::Success);
    assert_eq!(s.dn.as_deref(), Some("cn=joe,o=Acme"));
    assert!(!s.admin);
    assert_eq!(
        t.bind("cn=joe,o=Acme", "pw2").1.code,
        ResultCode::InvalidCredentials
    );
    assert_eq!(
        t.bind("cn=nobody,o=Acme", "pw").1.code,
        ResultCode::InvalidCredentials
    );
    assert_eq!(
        t.bind("cn=joe,o=Acme", "").1.code,
        ResultCode::UnwillingToPerform
    );
    assert_eq!(t.bind("", "").1.code, ResultCode::Success);
    assert_eq!(t.bind("not a dn", "x").1.code, ResultCode::InvalidDnSyntax);
    for mech in ["DIGEST-MD5", "CRAM-MD5", "GSSAPI", "OS400-PRFTKN"] {
        assert_eq!(
            bind_ext(&mut t, mech, &dirsrv::ops::Conn::default()).0,
            ResultCode::AuthMethodNotSupported
        );
    }
    let (c, _) = bind_ext(&mut t, "EXTERNAL", &dirsrv::ops::Conn::default());
    assert_eq!(c, ResultCode::InappropriateAuthentication);
    let conn = dirsrv::ops::Conn {
        tls: true,
        client_subject: Some("CN=Joe,O=Acme".into()),
    };
    let (c, s) = bind_ext(&mut t, "EXTERNAL", &conn);
    assert_eq!(c, ResultCode::Success);
    assert_eq!(s.dn.as_deref(), Some("cn=joe,o=Acme"));
    let conn = dirsrv::ops::Conn {
        tls: true,
        client_subject: Some("CN=Stranger,O=Acme".into()),
    };
    assert_eq!(
        bind_ext(&mut t, "EXTERNAL", &conn).0,
        ResultCode::InvalidCredentials
    );
    // Anonymous binds refused by the configuration.
    let mut st = harness::settings();
    st.allow_anon = false;
    let mut t = T::with(st);
    assert_eq!(
        t.bind("", "").1.code,
        ResultCode::InappropriateAuthentication
    );
    // Unset administrator password: 49.
    let mut st = harness::settings();
    st.admin_pw = None;
    let mut t = T::with(st);
    assert_eq!(
        t.bind("cn=Administrator", "").1.code,
        ResultCode::UnwillingToPerform
    );
    assert_eq!(
        t.bind("cn=Administrator", "anything").1.code,
        ResultCode::InvalidCredentials
    );
}

#[test]
fn stored_schemes_verify() {
    let mut t = T::new();
    let a = t.admin.clone();
    let stored = [
        ("c1", "{SHA}5en6G6MezRroT3XKqkdPOmY/BfQ="),
        (
            "c2",
            "{CRYPT}$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1",
        ),
        (
            "c3",
            "{CRYPT}$5$saltstring$5B8vYYiY.CVt1RlTTf8KbXBH3hsxY/GNooZaBBGWEc5",
        ),
        ("c4", "{CRYPT}$1$saltstri$YMyguxXMBpd2TEZ.vS/3q1"),
    ];
    for (cn, v) in stored {
        t.add_ok(
            &a,
            &format!("cn={cn},o=Acme"),
            &[
                text("objectClass", &["person"]),
                text("cn", &[cn]),
                text("sn", &["s"]),
                text("userPassword", &[v]),
            ],
        );
    }
    assert_eq!(t.bind("cn=c1,o=Acme", "secret").1.code, ResultCode::Success);
    for cn in ["c2", "c3", "c4"] {
        assert_eq!(
            t.bind(&format!("cn={cn},o=Acme"), "Hello world!").1.code,
            ResultCode::Success,
            "{cn}"
        );
    }
    // A clear value is hashed with the configured scheme; `none` stores
    // it as given.
    let mut s = harness::settings();
    s.pw_scheme = dirsrv::pwd::Scheme::Clear;
    let mut t = T::with(s);
    t.person("plain", "pw");
    let a = t.admin.clone();
    let r = t.search(
        &a,
        "cn=plain,o=Acme",
        Scope::Base,
        "(objectClass=*)",
        &["userPassword"],
    );
    assert_eq!(vals(&r, "userPassword"), ["pw"]);
    assert_eq!(t.bind("cn=plain,o=Acme", "pw").1.code, ResultCode::Success);
}

fn ext_op(
    t: &mut T,
    s: &dirsrv::ops::Session,
    name: &str,
    value: Option<Vec<u8>>,
) -> dirsrv::ops::Outcome {
    t.run(
        s,
        ProtocolOp::ExtendedRequest {
            name: name.into(),
            value,
        },
        Vec::new(),
    )
}

#[test]
fn extended_operations() {
    let mut t = T::new();
    t.person("joe", "pw1");
    let anon = dirsrv::ops::Session::anonymous();
    let o = ext_op(&mut t, &anon, ext::WHO_AM_I, None);
    let ProtocolOp::ExtendedResponse { result, value, .. } = &o.replies[0].op else {
        panic!()
    };
    assert_eq!(result.code, ResultCode::Success);
    assert_eq!(value.as_deref().unwrap_or_default(), b"");
    let (joe, _, _) = t.bind("cn=joe,o=Acme", "pw1");
    let o = ext_op(&mut t, &joe, ext::WHO_AM_I, None);
    let ProtocolOp::ExtendedResponse { value, .. } = &o.replies[0].op else {
        panic!()
    };
    assert_eq!(value.as_deref().unwrap(), b"dn:cn=joe,o=Acme");
    // StartTLS: the job's handshake.
    let o = ext_op(&mut t, &anon, ext::START_TLS, None);
    assert!(o.start_tls);
    assert_eq!(T::code(&o), ResultCode::Success);
    let o = ext_op(&mut t, &anon, ext::START_TLS, Some(vec![1]));
    assert_eq!(T::code(&o), ResultCode::ProtocolError);
    // Unknown and IBM's: protocolError.
    for oid in [
        "1.3.6.1.4.1.99999.9.9",
        "1.3.18.0.2.12.58",
        "1.3.18.0.2.12.5",
    ] {
        let o = ext_op(&mut t, &anon, oid, None);
        assert_eq!(T::code(&o), ResultCode::ProtocolError, "{oid}");
    }
    // Password modify: own, with the old one.
    let req = PasswordModifyRequest {
        user: None,
        old: Some(b"pw1".to_vec()),
        new: Some(b"pw2".to_vec()),
    };
    let o = ext_op(&mut t, &joe, ext::PASSWORD_MODIFY, Some(req.encode_value()));
    assert_eq!(T::code(&o), ResultCode::Success, "{:?}", o.replies[0].op);
    assert_eq!(t.bind("cn=joe,o=Acme", "pw2").1.code, ResultCode::Success);
    let req = PasswordModifyRequest {
        user: None,
        old: Some(b"wrong".to_vec()),
        new: Some(b"pw3".to_vec()),
    };
    let o = ext_op(&mut t, &joe, ext::PASSWORD_MODIFY, Some(req.encode_value()));
    assert_eq!(T::code(&o), ResultCode::InvalidCredentials);
    // The administrator generates one for joe.
    let a = t.admin.clone();
    let req = PasswordModifyRequest {
        user: Some("cn=joe,o=Acme".into()),
        old: None,
        new: None,
    };
    let o = ext_op(&mut t, &a, ext::PASSWORD_MODIFY, Some(req.encode_value()));
    assert_eq!(T::code(&o), ResultCode::Success);
    let ProtocolOp::ExtendedResponse { value, .. } = &o.replies[0].op else {
        panic!()
    };
    let generated = PasswordModifyResponse::decode_value(value.as_deref())
        .unwrap()
        .generated
        .unwrap();
    assert_eq!(
        t.bind("cn=joe,o=Acme", &String::from_utf8(generated).unwrap())
            .1
            .code,
        ResultCode::Success
    );
    // Anonymous, for itself: refused.
    let req = PasswordModifyRequest {
        user: None,
        old: None,
        new: Some(b"x".to_vec()),
    };
    let o = ext_op(
        &mut t,
        &anon,
        ext::PASSWORD_MODIFY,
        Some(req.encode_value()),
    );
    assert_eq!(T::code(&o), ResultCode::UnwillingToPerform);
}

#[test]
fn proxied_authorization() {
    let mut t = T::new();
    t.person("joe", "pw1");
    let a = t.admin.clone();
    let proxy = |id: &str| ldap::control::ProxiedAuthz(id.into()).to_control(true);
    let o = t.run(
        &a,
        ProtocolOp::ExtendedRequest {
            name: ext::WHO_AM_I.into(),
            value: None,
        },
        vec![proxy("dn:cn=joe,o=Acme")],
    );
    // Whoami runs as the identity.
    assert_eq!(T::code(&o), ResultCode::Success);
    let ProtocolOp::ExtendedResponse { value, .. } = &o.replies[0].op else {
        panic!()
    };
    assert_eq!(value.as_deref(), Some(&b"dn:cn=joe,o=Acme"[..]));
    // As joe, the administrator cannot read joe's password.
    let r = t.search_req(
        &a,
        T::req(
            "cn=joe,o=Acme",
            Scope::Base,
            "(objectClass=*)",
            &["userPassword", "cn"],
        ),
        vec![proxy("dn:cn=joe,o=Acme")],
    );
    assert_eq!(names(&r), ["cn"]);
    let r = t.search_req(
        &a,
        T::req("cn=joe,o=Acme", Scope::Base, "(objectClass=*)", &[]),
        vec![proxy("u:joe")],
    );
    assert_eq!(done(&r).code, ResultCode::Other(123));
    let (joe, _, _) = t.bind("cn=joe,o=Acme", "pw1");
    let r = t.search_req(
        &joe,
        T::req("cn=joe,o=Acme", Scope::Base, "(objectClass=*)", &[]),
        vec![proxy("dn:cn=Administrator")],
    );
    assert_eq!(done(&r).code, ResultCode::Other(123));
    let anon = dirsrv::ops::Session::anonymous();
    let r = t.search_req(
        &anon,
        T::req("cn=joe,o=Acme", Scope::Base, "(objectClass=*)", &[]),
        vec![proxy("dn:cn=Administrator")],
    );
    assert_eq!(done(&r).code, ResultCode::InsufficientAccessRights);
    let r = t.search_req(
        &a,
        T::req("cn=joe,o=Acme", Scope::Base, "(objectClass=*)", &[]),
        vec![ldap::control::ProxiedAuthz("dn:cn=joe,o=Acme".into()).to_control(false)],
    );
    assert_eq!(done(&r).code, ResultCode::ProtocolError);
}

#[test]
fn change_log() {
    let mut s = harness::settings();
    s.changelog_max = 3;
    let mut t = T::with(s);
    let a = t.admin.clone();
    ou(&mut t, "A", "o=Acme");
    assert_eq!(
        modify(
            &mut t,
            &a,
            "ou=A,o=Acme",
            vec![m(ModOp::Add, "description", &["d"])]
        ),
        ResultCode::Success
    );
    assert_eq!(
        moddn(&mut t, &a, "ou=A,o=Acme", "ou=B", true, None),
        ResultCode::Success
    );
    assert_eq!(
        delete(&mut t, &a, "ou=B,o=Acme", Vec::new()),
        ResultCode::Success
    );
    let r = t.search(
        &a,
        "cn=changelog",
        Scope::One,
        "(objectClass=*)",
        &["changeType", "targetDN", "changeNumber"],
    );
    let kinds: Vec<String> = r
        .iter()
        .filter_map(|x| match &x.op {
            ProtocolOp::SearchResultEntry { attrs, .. } => attrs
                .iter()
                .find(|a| a.ty == "changeType")
                .map(|a| String::from_utf8_lossy(&a.vals[0]).to_string()),
            _ => None,
        })
        .collect();
    // Trimmed to the last three.
    assert_eq!(kinds, ["modify", "modrdn", "delete"]);
    let r = t.search(
        &a,
        "cn=changelog",
        Scope::Sub,
        "(changeType=modify)",
        &["changes"],
    );
    assert_eq!(dns(&r).len(), 1);
    assert!(vals(&r, "changes")[0].contains("description: d"));
    let r = t.search(&a, "cn=changelog", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(dns(&r), ["cn=changelog"]);
    let first = dns(&t.search(&a, "cn=changelog", Scope::One, "(objectClass=*)", &[]))[0].clone();
    assert_eq!(
        delete(&mut t, &a, &first, Vec::new()),
        ResultCode::UnwillingToPerform
    );
}

#[test]
fn persistent_search() {
    let mut t = T::new();
    let a = t.admin.clone();
    let anon = dirsrv::ops::Session::anonymous();
    let ps = PersistentSearch {
        change_types: change_type::ALL,
        changes_only: true,
        return_ecs: true,
    };
    let msg = T::msg(
        ProtocolOp::SearchRequest(T::req(
            "o=Acme",
            Scope::Sub,
            "(objectClass=organizationalUnit)",
            &["ou"],
        )),
        vec![ps.to_control(true)],
    );
    let mut txn = t.sys.store.begin().unwrap();
    let dirsrv::ops::Started::Cursor(id) = t.dir.search(&mut txn, &anon, &msg, &mut t.cursors)
    else {
        panic!()
    };
    let p = t
        .dir
        .next(&mut txn, &mut t.cursors, id, dirsrv::ops::Budget::default());
    drop(txn);
    let dirsrv::ops::PageState::Watching(w) = p.state else {
        panic!("{:?}", p.state)
    };
    assert!(p.replies.is_empty());
    ou(&mut t, "New", "o=Acme");
    t.person("noise", "x");
    assert_eq!(
        moddn(&mut t, &a, "ou=New,o=Acme", "ou=Newer", true, None),
        ResultCode::Success
    );
    let got = t.dir.changes(w, 10).unwrap();
    assert_eq!(got.len(), 2);
    let ec = EntryChange::find(&got[0].controls).unwrap().unwrap();
    assert_eq!(ec.change_type, change_type::ADD);
    let ec = EntryChange::find(&got[1].controls).unwrap().unwrap();
    assert_eq!(ec.change_type, change_type::MODDN);
    assert_eq!(ec.previous_dn.as_deref(), Some("ou=New,o=Acme"));
    assert!(t.dir.changes(w, 10).unwrap().is_empty());
    t.dir.close_watch(w);
    assert!(t.dir.changes(w, 10).is_none());
    // Not every change type.
    let ps = PersistentSearch {
        change_types: change_type::DELETE,
        changes_only: false,
        return_ecs: false,
    };
    let msg = T::msg(
        ProtocolOp::SearchRequest(T::req("o=Acme", Scope::One, "(ou=*)", &["ou"])),
        vec![ps.to_control(true)],
    );
    let mut txn = t.sys.store.begin().unwrap();
    let dirsrv::ops::Started::Cursor(id) = t.dir.search(&mut txn, &anon, &msg, &mut t.cursors)
    else {
        panic!()
    };
    let p = t
        .dir
        .next(&mut txn, &mut t.cursors, id, dirsrv::ops::Budget::default());
    drop(txn);
    assert_eq!(dns(&p.replies), ["ou=Newer,o=Acme"]);
    let dirsrv::ops::PageState::Watching(w) = p.state else {
        panic!()
    };
    assert_eq!(
        modify(
            &mut t,
            &a,
            "ou=Newer,o=Acme",
            vec![m(ModOp::Add, "description", &["x"])]
        ),
        ResultCode::Success
    );
    assert!(t.dir.changes(w, 10).unwrap().is_empty());
    assert_eq!(
        delete(&mut t, &a, "ou=Newer,o=Acme", Vec::new()),
        ResultCode::Success
    );
    assert_eq!(t.dir.changes(w, 10).unwrap().len(), 1);
    // Off by configuration: the control is unknown.
    let mut s = harness::settings();
    s.psearch = false;
    let mut t = T::with(s);
    let r = t.search_req(
        &anon,
        T::req("o=Acme", Scope::Sub, "(objectClass=*)", &[]),
        vec![ps.to_control(true)],
    );
    assert_eq!(done(&r).code, ResultCode::UnavailableCriticalExtension);
}

#[test]
fn read_only_and_schema_writes() {
    let mut s = harness::settings();
    s.read_only = true;
    let mut t = T::with(s);
    let a = t.admin.clone();
    assert_eq!(
        modify(
            &mut t,
            &a,
            "o=Acme",
            vec![m(ModOp::Add, "description", &["x"])]
        ),
        ResultCode::UnwillingToPerform
    );
    let mut t = T::new();
    let a = t.admin.clone();
    let def = "( 1.3.6.1.4.1.99999.1 NAME 'qsTestAttr' SYNTAX 1.3.6.1.4.1.1466.115.121.1.15 )";
    let o = t.run(
        &a,
        ProtocolOp::ModifyRequest {
            dn: "cn=schema".into(),
            changes: vec![m(ModOp::Add, "attributeTypes", &[def])],
        },
        Vec::new(),
    );
    assert_eq!(T::code(&o), ResultCode::Success);
    assert!(o.schema_changed);
    let anon = dirsrv::ops::Session::anonymous();
    let o = t.run(
        &anon,
        ProtocolOp::ModifyRequest {
            dn: "cn=schema".into(),
            changes: vec![m(
                ModOp::Add,
                "attributeTypes",
                &["( 1.3.6.1.4.1.99999.2 NAME 'qsOther' SYNTAX 1.3.6.1.4.1.1466.115.121.1.15 )"],
            )],
        },
        Vec::new(),
    );
    assert_eq!(T::code(&o), ResultCode::InsufficientAccessRights);
    assert_eq!(
        modify(&mut t, &a, "", vec![m(ModOp::Add, "description", &["x"])]),
        ResultCode::UnwillingToPerform
    );
    assert_eq!(
        modify(
            &mut t,
            &a,
            "cn=monitor",
            vec![m(ModOp::Add, "description", &["x"])]
        ),
        ResultCode::UnwillingToPerform
    );
}

#[test]
fn monitor_counts() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    t.bind("", "");
    let r = t.search(
        &anon,
        "cn=monitor",
        Scope::Base,
        "(objectClass=*)",
        &["bindsrequested", "addscompleted", "starttime"],
    );
    assert_eq!(vals(&r, "bindsrequested"), ["1"]);
    assert!(vals(&r, "addscompleted")[0].parse::<u32>().unwrap() >= 6);
    assert_eq!(vals(&r, "starttime"), ["20261113120000.000000Z"]);
}

#[test]
fn policy_response_on_bind() {
    let mut t = T::new();
    t.person("joe", "pw1");
    let a = t.admin.clone();
    assert_eq!(
        modify(
            &mut t,
            &a,
            "cn=pwdpolicy,cn=ibmpolicies",
            vec![
                m(ModOp::Replace, "ibm-pwdPolicy", &["TRUE"]),
                m(ModOp::Replace, "pwdLockout", &["TRUE"]),
                m(ModOp::Replace, "pwdMaxFailure", &["2"]),
                m(ModOp::Replace, "pwdLockoutDuration", &["0"]),
            ]
        ),
        ResultCode::Success
    );
    assert_eq!(
        t.bind("cn=joe,o=Acme", "bad").1.code,
        ResultCode::InvalidCredentials
    );
    assert_eq!(
        t.bind("cn=joe,o=Acme", "bad").1.code,
        ResultCode::InvalidCredentials
    );
    let (_, r, ctls) = t.bind("cn=joe,o=Acme", "pw1");
    assert_eq!(r.code, ResultCode::InvalidCredentials);
    let p = PasswordPolicyResponse::find(&ctls).unwrap().unwrap();
    assert_eq!(p.error, Some(PolicyError::AccountLocked));
    // The administrator unlocks.
    assert_eq!(
        modify(
            &mut t,
            &a,
            "cn=joe,o=Acme",
            vec![m(ModOp::Replace, "pwdAccountLockedTime", &[])]
        ),
        ResultCode::Success
    );
    assert_eq!(t.bind("cn=joe,o=Acme", "pw1").1.code, ResultCode::Success);
}

/// What ldaptest's `assertion-write` does, then more adds: a false
/// assertion's modify and delete leave the store whole.
#[test]
fn false_assertions_leave_the_store_whole() {
    let mut t = T::new();
    let a = t.admin.clone();
    ou(&mut t, "Sb", "o=Acme");
    t.add_ok(
        &a,
        "cn=a,ou=Sb,o=Acme",
        &[
            text("objectClass", &["top", "person"]),
            text("cn", &["a"]),
            text("sn", &["one"]),
        ],
    );
    let assert = |f: &str| {
        vec![ldap::control::Assertion(ldap::Filter::parse(f).unwrap()).to_control(true)]
    };
    let o = t.run(
        &a,
        ProtocolOp::ModifyRequest {
            dn: "cn=a,ou=Sb,o=Acme".into(),
            changes: vec![m(ModOp::Replace, "sn", &["two"])],
        },
        assert("(sn=other)"),
    );
    assert_eq!(T::code(&o), ResultCode::Other(122));
    let o = t.run(
        &a,
        ProtocolOp::ModifyRequest {
            dn: "cn=a,ou=Sb,o=Acme".into(),
            changes: vec![m(ModOp::Replace, "sn", &["two"])],
        },
        assert("(sn=one)"),
    );
    assert_eq!(T::code(&o), ResultCode::Success);
    let o = t.run(
        &a,
        ProtocolOp::DelRequest("cn=a,ou=Sb,o=Acme".into()),
        assert("(sn=one)"),
    );
    assert_eq!(T::code(&o), ResultCode::Other(122));
    for dn in ["cn=a,ou=Sb,o=Acme", "ou=Sb,o=Acme"] {
        let o = t.run(&a, ProtocolOp::DelRequest(dn.into()), Vec::new());
        assert_eq!(T::code(&o), ResultCode::Success, "delete {dn}");
    }
    for i in 0..20 {
        ou(&mut t, &format!("After{i}"), "o=Acme");
    }
}

/// An anonymous subtree search meets a referral object: a continuation
/// reference.
#[test]
fn anonymous_searches_get_continuation_references() {
    let mut t = T::new();
    let a = t.admin.clone();
    ou(&mut t, "Sb", "o=Acme");
    t.add_ok(
        &a,
        "ou=ref,ou=Sb,o=Acme",
        &[
            text("objectClass", &["top", "referral", "extensibleObject"]),
            text("ou", &["ref"]),
            text("ref", &["ldap://ldaptest.invalid/ou=far,dc=example"]),
        ],
    );
    let anon = dirsrv::ops::Session::anonymous();
    let r = t.search(&anon, "ou=Sb,o=Acme", Scope::Sub, "(objectClass=*)", &["1.1"]);
    assert!(
        r.iter()
            .any(|x| matches!(&x.op, ProtocolOp::SearchResultReference(_))),
        "{r:?}"
    );
}
