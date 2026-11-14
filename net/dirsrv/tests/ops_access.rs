// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Access control and password policy through the operations, and the
//! service's hooks (identity, audit, configuration) with test doubles.

mod harness;

use std::sync::{Arc, Mutex};

use dirsrv::backend::config::{ConfigBackend, ConfigSource};
use dirsrv::ops::{Audit, AuditEvent, AuditOp, Granted, Identity, Refused, Session};
use dirsrv::schema::Schema;
use harness::{T, dns, done, names, text, vals};
use ldap::control::{ControlValue, PasswordPolicyResponse, PolicyError, PolicyWarning};
use ldap::msg::{Attribute, ModOp, Modification, ProtocolOp, Scope};
use ldap::{Dn, ResultCode};
use store::txn::Txn;

fn m(op: ModOp, ty: &str, vals: &[&str]) -> Modification {
    Modification {
        op,
        attr: Attribute::new(ty, vals),
    }
}

fn modify(t: &mut T, s: &Session, dn: &str, mods: Vec<Modification>) -> dirsrv::ops::Outcome {
    t.run(
        s,
        ProtocolOp::ModifyRequest {
            dn: dn.into(),
            changes: mods,
        },
        vec![ldap::control::Control::new(ldap::control::PASSWORD_POLICY, false, None)],
    )
}

fn code(o: &dirsrv::ops::Outcome) -> ResultCode {
    T::code(o)
}

/// o=Acme with ou=Eng (an ACL for the eng group, propagating) and
/// ou=Eng/ou=Lab (an ACL that does not propagate), users and a group.
fn world() -> (T, Session, Session, Session) {
    let mut t = T::new();
    let a = t.admin.clone();
    for (cn, pw) in [("ann", "pwa"), ("bob", "pwb"), ("eve", "pwe")] {
        t.person(cn, pw);
    }
    t.add_ok(
        &a,
        "cn=eng,o=Acme",
        &[text("objectClass", &["groupOfNames"]), text("cn", &["eng"]), text("member", &["cn=ann,o=Acme", "CN=BOB, O=ACME"])],
    );
    t.add_ok(
        &a,
        "ou=Eng,o=Acme",
        &[
            text("objectClass", &["organizationalUnit"]),
            text("ou", &["Eng"]),
            text(
                "aclEntry",
                &[
                    "group:cn=eng,o=Acme:object:a:normal:rwsc",
                    "access-id:cn=bob,o=Acme:normal:deny:w",
                    "group:cn=anybody:normal:rs",
                ],
            ),
            text("entryOwner", &["access-id:cn=eve,o=Acme"]),
        ],
    );
    t.add_ok(
        &a,
        "ou=Lab,ou=Eng,o=Acme",
        &[
            text("objectClass", &["organizationalUnit"]),
            text("ou", &["Lab"]),
            text("aclEntry", &["group:cn=anybody:normal:r"]),
            text("aclPropagate", &["FALSE"]),
        ],
    );
    t.add_ok(
        &a,
        "cn=dev,ou=Lab,ou=Eng,o=Acme",
        &[text("objectClass", &["person"]), text("cn", &["dev"]), text("sn", &["d"]), text("description", &["x"])],
    );
    let ann = t.bind("cn=ann,o=Acme", "pwa").0;
    let bob = t.bind("cn=bob,o=Acme", "pwb").0;
    let eve = t.bind("cn=eve,o=Acme", "pwe").0;
    (t, ann, bob, eve)
}

#[test]
fn groups_found_at_bind() {
    let (t, ann, bob, eve) = world();
    let key = |s: &str| t.dir.schema.normalize_parsed_dn(&Dn::parse(s).unwrap());
    assert_eq!(ann.groups, [key("cn=eng,o=Acme")]);
    assert_eq!(bob.groups, [key("cn=eng,o=Acme")]);
    assert!(eve.groups.is_empty());
}

#[test]
fn inherited_acl_and_owner() {
    let (mut t, ann, bob, eve) = world();
    let anon = Session::anonymous();
    // Lab has its own ACL, which does not propagate: cn=dev below it
    // inherits ou=Eng's.
    let adm = t.admin.clone();
    let r = t.search(&adm, "cn=dev,ou=Lab,ou=Eng,o=Acme", Scope::Base, "(objectClass=*)", &["aclSource", "ownerSource", "ibm-effectiveAcl"]);
    assert_eq!(vals(&r, "aclSource"), ["ou=Eng,o=Acme"]);
    assert_eq!(vals(&r, "ownerSource"), ["ou=Eng,o=Acme"]);
    let r = t.search(&adm, "ou=Lab,ou=Eng,o=Acme", Scope::Base, "(objectClass=*)", &["aclSource", "aclPropagate"]);
    assert_eq!(vals(&r, "aclSource"), ["ou=Lab,ou=Eng,o=Acme"]);
    assert_eq!(vals(&r, "aclPropagate"), ["FALSE"]);
    let dev = "cn=dev,ou=Lab,ou=Eng,o=Acme";
    // Ann (group eng) writes; Bob is in eng but denied w by access-id;
    // anonymous reads but cannot compare (no c), the owner does all.
    let o = modify(&mut t, &ann, dev, vec![m(ModOp::Replace, "description", &["by ann"])]);
    assert_eq!(code(&o), ResultCode::Success);
    let o = modify(&mut t, &bob, dev, vec![m(ModOp::Replace, "description", &["by bob"])]);
    assert_eq!(code(&o), ResultCode::InsufficientAccessRights);
    let o = modify(&mut t, &anon, dev, vec![m(ModOp::Replace, "description", &["anon"])]);
    assert_eq!(code(&o), ResultCode::InsufficientAccessRights);
    let r = t.search(&anon, dev, Scope::Base, "(objectClass=*)", &["description"]);
    assert_eq!(vals(&r, "description"), ["by ann"]);
    let cmp = |t: &mut T, s: &Session| {
        code(&t.run(
            s,
            ProtocolOp::CompareRequest {
                dn: dev.into(),
                attr: "description".into(),
                value: b"by ann".to_vec(),
            },
            Vec::new(),
        ))
    };
    assert_eq!(cmp(&mut t, &anon), ResultCode::InsufficientAccessRights);
    assert_eq!(cmp(&mut t, &ann), ResultCode::CompareTrue);
    let o = modify(&mut t, &eve, dev, vec![m(ModOp::Replace, "description", &["by eve"])]);
    assert_eq!(code(&o), ResultCode::Success);
    // Adds below need object:a (eng has it), deletes object:d (owner).
    let o = t.add(&ann, "cn=new,ou=Eng,o=Acme", &[text("objectClass", &["person"]), text("cn", &["new"]), text("sn", &["n"])]);
    assert_eq!(code(&o), ResultCode::Success);
    let o = t.run(&ann, ProtocolOp::DelRequest("cn=new,ou=Eng,o=Acme".into()), Vec::new());
    assert_eq!(code(&o), ResultCode::InsufficientAccessRights);
    let o = t.run(&eve, ProtocolOp::DelRequest("cn=new,ou=Eng,o=Acme".into()), Vec::new());
    assert_eq!(code(&o), ResultCode::Success);
    // Filters on attributes without `s` are Undefined: Lab grants r only.
    let r = t.search(&anon, "ou=Lab,ou=Eng,o=Acme", Scope::Base, "(ou=Lab)", &[]);
    assert!(dns(&r).is_empty());
    let r = t.search(&anon, "ou=Eng,o=Acme", Scope::Base, "(ou=Eng)", &[]);
    assert_eq!(dns(&r), ["ou=Eng,o=Acme"]);
}

#[test]
fn acl_attributes_need_restricted_rights() {
    let (mut t, ann, _bob, eve) = world();
    let dev = "cn=dev,ou=Lab,ou=Eng,o=Acme";
    let o = modify(&mut t, &ann, dev, vec![m(ModOp::Add, "aclEntry", &["group:cn=anybody:normal:rwsc"])]);
    assert_eq!(code(&o), ResultCode::InsufficientAccessRights);
    let o = modify(&mut t, &eve, dev, vec![m(ModOp::Add, "aclEntry", &["group:cn=anybody:normal:rwsc"])]);
    assert_eq!(code(&o), ResultCode::Success);
    let o = modify(&mut t, &eve, dev, vec![m(ModOp::Add, "aclEntry", &["not an acl"])]);
    assert_eq!(code(&o), ResultCode::InvalidAttributeSyntax);
    let o = modify(&mut t, &eve, dev, vec![m(ModOp::Add, "ibm-filterAclEntry", &["x"])]);
    assert_eq!(code(&o), ResultCode::UnwillingToPerform);
    let anon = Session::anonymous();
    let o = modify(&mut t, &anon, dev, vec![m(ModOp::Replace, "description", &["anon"])]);
    assert_eq!(code(&o), ResultCode::Success);
}

#[test]
fn passwords_are_critical() {
    let (mut t, ann, bob, _eve) = world();
    // Nobody but the owner and the administrator reads userPassword.
    let r = t.search(&bob, "cn=ann,o=Acme", Scope::Base, "(objectClass=*)", &["userPassword", "cn"]);
    assert_eq!(names(&r), ["cn"]);
    let a = t.admin.clone();
    let r = t.search(&a, "cn=ann,o=Acme", Scope::Base, "(objectClass=*)", &["userPassword"]);
    assert_eq!(vals(&r, "userPassword").len(), 1);
    // A user changes its own password without an ACL saying so.
    let o = modify(&mut t, &ann, "cn=ann,o=Acme", vec![m(ModOp::Replace, "userPassword", &["new-a"])]);
    assert_eq!(code(&o), ResultCode::Success);
    assert_eq!(t.bind("cn=ann,o=Acme", "new-a").1.code, ResultCode::Success);
    let o = modify(&mut t, &bob, "cn=ann,o=Acme", vec![m(ModOp::Replace, "userPassword", &["mine"])]);
    assert_eq!(code(&o), ResultCode::InsufficientAccessRights);
}

fn policy_on(t: &mut T, mods: &[(&str, &str)]) {
    let a = t.admin.clone();
    let mut v = vec![m(ModOp::Replace, "ibm-pwdPolicy", &["TRUE"])];
    for (k, x) in mods {
        v.push(m(ModOp::Replace, k, &[x]));
    }
    let o = modify(t, &a, "cn=pwdpolicy,cn=ibmpolicies", v);
    assert_eq!(code(&o), ResultCode::Success, "{:?}", o.replies[0].op);
}

fn policy_error(o: &dirsrv::ops::Outcome) -> Option<PolicyError> {
    PasswordPolicyResponse::find(&o.replies[0].controls).ok().flatten().and_then(|p| p.error)
}

#[test]
fn policy_quality_history_and_reset() {
    let mut t = T::new();
    t.person("joe", "first-pw");
    policy_on(
        &mut t,
        &[("pwdCheckSyntax", "1"), ("pwdMinLength", "6"), ("pwdInHistory", "2"), ("pwdMustChange", "TRUE")],
    );
    let joe = t.bind("cn=joe,o=Acme", "first-pw").0;
    let o = modify(&mut t, &joe, "cn=joe,o=Acme", vec![m(ModOp::Replace, "userPassword", &["short"])]);
    assert_eq!(code(&o), ResultCode::ConstraintViolation);
    assert_eq!(policy_error(&o), Some(PolicyError::PasswordTooShort));
    let o = modify(&mut t, &joe, "cn=joe,o=Acme", vec![m(ModOp::Replace, "userPassword", &["first-pw"])]);
    assert_eq!(policy_error(&o), Some(PolicyError::PasswordInHistory));
    let o = modify(&mut t, &joe, "cn=joe,o=Acme", vec![m(ModOp::Replace, "userPassword", &["second-pw"])]);
    assert_eq!(code(&o), ResultCode::Success);
    let o = modify(&mut t, &joe, "cn=joe,o=Acme", vec![m(ModOp::Replace, "userPassword", &["first-pw"])]);
    assert_eq!(policy_error(&o), Some(PolicyError::PasswordInHistory));
    // A reset by the administrator: the next bind must change it.
    let a = t.admin.clone();
    let o = modify(&mut t, &a, "cn=joe,o=Acme", vec![m(ModOp::Replace, "userPassword", &["reset-pw"])]);
    assert_eq!(code(&o), ResultCode::Success);
    let (joe, r, ctls) = t.bind("cn=joe,o=Acme", "reset-pw");
    assert_eq!(r.code, ResultCode::Success);
    assert_eq!(PasswordPolicyResponse::find(&ctls).unwrap().unwrap().error, Some(PolicyError::ChangeAfterReset));
    assert!(joe.must_change);
    let o = modify(&mut t, &joe, "cn=joe,o=Acme", vec![m(ModOp::Replace, "description", &["x"])]);
    assert_eq!(code(&o), ResultCode::UnwillingToPerform);
    assert_eq!(policy_error(&o), Some(PolicyError::ChangeAfterReset));
    let o = modify(&mut t, &joe, "cn=joe,o=Acme", vec![m(ModOp::Replace, "userPassword", &["third-pw"])]);
    assert_eq!(code(&o), ResultCode::Success);
    let (joe, _, ctls) = t.bind("cn=joe,o=Acme", "third-pw");
    assert!(!joe.must_change);
    assert!(PasswordPolicyResponse::find(&ctls).unwrap().is_none());
}

#[test]
fn policy_expiry_and_grace() {
    let mut t = T::new();
    policy_on(&mut t, &[("pwdMaxAge", "100"), ("pwdExpireWarning", "50"), ("pwdGraceLoginLimit", "1")]);
    t.person("joe", "pw-joe");
    t.advance(60);
    let (_, r, ctls) = t.bind("cn=joe,o=Acme", "pw-joe");
    assert_eq!(r.code, ResultCode::Success);
    assert_eq!(PasswordPolicyResponse::find(&ctls).unwrap().unwrap().warning, Some(PolicyWarning::TimeBeforeExpiration(40)));
    t.advance(100);
    let (_, r, ctls) = t.bind("cn=joe,o=Acme", "pw-joe");
    assert_eq!(r.code, ResultCode::Success);
    assert_eq!(PasswordPolicyResponse::find(&ctls).unwrap().unwrap().warning, Some(PolicyWarning::GraceAuthNsRemaining(0)));
    t.advance(1);
    let (_, r, ctls) = t.bind("cn=joe,o=Acme", "pw-joe");
    assert_eq!(r.code, ResultCode::InvalidCredentials);
    assert_eq!(PasswordPolicyResponse::find(&ctls).unwrap().unwrap().error, Some(PolicyError::PasswordExpired));
}

#[test]
fn individual_and_group_policies() {
    let mut t = T::new();
    let a = t.admin.clone();
    policy_on(&mut t, &[("ibm-pwdGroupAndIndividualEnabled", "TRUE"), ("pwdMinLength", "4"), ("pwdCheckSyntax", "1")]);
    let pol = |t: &mut T, cn: &str, len: &str| {
        let a = t.admin.clone();
        t.add_ok(
            &a,
            &format!("cn={cn},cn=ibmpolicies"),
            &[
                text("objectClass", &["container", "pwdPolicy", "ibm-pwdPolicyExt"]),
                text("cn", &[cn]),
                text("pwdAttribute", &["userPassword"]),
                text("ibm-pwdPolicy", &["TRUE"]),
                text("pwdMinLength", &[len]),
            ],
        );
    };
    pol(&mut t, "long", "12");
    pol(&mut t, "mid", "8");
    t.person("ind", "pw-ind-1");
    t.person("grp", "pw-grp-1");
    let o = modify(&mut t, &a, "cn=ind,o=Acme", vec![m(ModOp::Add, "objectClass", &["ibm-pwdIndividualPolicy"]), m(ModOp::Add, "ibm-pwdIndividualPolicyDN", &["cn=long,cn=ibmpolicies"])]);
    if code(&o) != ResultCode::Success {
        // Without the auxiliary class the attribute needs extensibleObject.
        let o = modify(&mut t, &a, "cn=ind,o=Acme", vec![m(ModOp::Add, "objectClass", &["extensibleObject"]), m(ModOp::Add, "ibm-pwdIndividualPolicyDN", &["cn=long,cn=ibmpolicies"])]);
        assert_eq!(code(&o), ResultCode::Success, "{:?}", o.replies[0].op);
    }
    t.add_ok(
        &a,
        "cn=staff,o=Acme",
        &[text("objectClass", &["groupOfNames", "extensibleObject"]), text("cn", &["staff"]), text("member", &["cn=grp,o=Acme"]), text("ibm-pwdGroupPolicyDN", &["cn=mid,cn=ibmpolicies"])],
    );
    let ind = t.bind("cn=ind,o=Acme", "pw-ind-1").0;
    let grp = t.bind("cn=grp,o=Acme", "pw-grp-1").0;
    let o = modify(&mut t, &ind, "cn=ind,o=Acme", vec![m(ModOp::Replace, "userPassword", &["ten-chars!"])]);
    assert_eq!(policy_error(&o), Some(PolicyError::PasswordTooShort));
    let o = modify(&mut t, &grp, "cn=grp,o=Acme", vec![m(ModOp::Replace, "userPassword", &["seven77"])]);
    assert_eq!(policy_error(&o), Some(PolicyError::PasswordTooShort));
    let o = modify(&mut t, &grp, "cn=grp,o=Acme", vec![m(ModOp::Replace, "userPassword", &["eight888"])]);
    assert_eq!(code(&o), ResultCode::Success);
}

/// The service's identity hook, as a double: `os400-profile=NAME,
/// cn=accounts,os400-sys=SYSTEM` binds with the password `NAME-pw`.
struct Profiles;

impl Identity for Profiles {
    fn owns(&self, schema: &Schema, dn: &Dn) -> bool {
        schema.dn_within(dn, &Dn::parse("cn=accounts,os400-sys=SYSTEM").unwrap())
    }
    fn bind(&mut self, _txn: &mut Txn<'_>, dn: &Dn, password: &[u8]) -> Result<Granted, Refused> {
        let name = dn.rdn().and_then(|r| r.0[0].value_str()).unwrap_or_default().to_string();
        if name == "LOCKED" {
            return Err(Refused {
                code: ResultCode::InvalidCredentials,
                policy: Some(PolicyError::AccountLocked),
            });
        }
        if password == format!("{name}-pw").as_bytes() {
            Ok(Granted {
                groups: vec!["cn=eng,o=Acme".into()],
                admin: name == "QSECOFR",
                warning: None,
            })
        } else {
            Err(Refused {
                code: ResultCode::InvalidCredentials,
                policy: None,
            })
        }
    }
    fn external(&mut self, _txn: &mut Txn<'_>, _dn: &Dn) -> Option<Granted> {
        None
    }
}

#[test]
fn identity_hook() {
    let mut t = T::new();
    t.dir.set_identity(Box::new(Profiles));
    let (s, r, _) = t.bind("os400-profile=QSECOFR,cn=accounts,os400-sys=SYSTEM", "QSECOFR-pw");
    assert_eq!(r.code, ResultCode::Success);
    assert!(s.admin);
    let (s, r, _) = t.bind("os400-profile=QSTEST,cn=accounts,os400-sys=SYSTEM", "QSTEST-pw");
    assert_eq!(r.code, ResultCode::Success);
    assert!(!s.admin);
    assert_eq!(s.groups.len(), 1);
    assert_eq!(t.bind("os400-profile=QSTEST,cn=accounts,os400-sys=SYSTEM", "bad").1.code, ResultCode::InvalidCredentials);
    let (_, r, ctls) = t.bind("os400-profile=LOCKED,cn=accounts,os400-sys=SYSTEM", "x");
    assert_eq!(r.code, ResultCode::InvalidCredentials);
    assert_eq!(PasswordPolicyResponse::find(&ctls).unwrap().unwrap().error, Some(PolicyError::AccountLocked));
}

struct Recorder(Arc<Mutex<Vec<AuditEvent>>>);

impl Audit for Recorder {
    fn record(&mut self, ev: &AuditEvent) {
        self.0.lock().unwrap().push(ev.clone());
    }
}

#[test]
fn audit_hook() {
    let mut t = T::new();
    let log = Arc::new(Mutex::new(Vec::new()));
    t.dir.set_audit(Box::new(Recorder(log.clone())));
    t.person("joe", "pw");
    let _ = t.bind("cn=joe,o=Acme", "bad");
    let anon = Session::anonymous();
    let _ = t.search(&anon, "o=Acme", Scope::Base, "(objectClass=*)", &[]);
    let v = log.lock().unwrap().clone();
    assert_eq!(v[0].op, AuditOp::Add);
    assert_eq!(v[1].op, AuditOp::Bind);
    assert_eq!(v[1].code, ResultCode::InvalidCredentials);
    assert_eq!(v[1].target, "cn=joe,o=Acme");
    assert_eq!(v[2].op, AuditOp::Search);
}

/// The configuration file as a double.
struct Conf(Vec<(String, Vec<Attribute>)>);

impl ConfigSource for Conf {
    fn entries(&self) -> Vec<(String, Vec<Attribute>)> {
        self.0.clone()
    }
    fn modify(&mut self, dn: &str, mods: &[Modification]) -> Result<(), (ResultCode, String)> {
        let e = self.0.iter_mut().find(|(d, _)| d.eq_ignore_ascii_case(dn)).ok_or((ResultCode::NoSuchObject, "no".into()))?;
        for m in mods {
            e.1.retain(|a| !a.ty.eq_ignore_ascii_case(&m.attr.ty));
            e.1.push(m.attr.clone());
        }
        Ok(())
    }
}

#[test]
fn configuration_backend() {
    let mut t = T::new();
    t.dir.add_backend(Box::new(ConfigBackend::new(Conf(vec![
        ("cn=Configuration".into(), vec![text("objectClass", &["top", "container"]), text("cn", &["Configuration"])]),
        (
            "cn=Front End,cn=Configuration".into(),
            vec![text("objectClass", &["top", "ibm-slapdFrontEnd"]), text("cn", &["Front End"]), text("ibm-slapdIdleTimeOut", &["300"])],
        ),
    ]))));
    let anon = Session::anonymous();
    let r = t.search(&anon, "cn=configuration", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::InsufficientAccessRights);
    let r = t.search(&anon, "cn=configuration", Scope::One, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::InsufficientAccessRights);
    let a = t.admin.clone();
    let r = t.search(&a, "cn=configuration", Scope::Sub, "(objectClass=*)", &["1.1"]);
    assert_eq!(dns(&r), ["cn=Configuration", "cn=Front End,cn=Configuration"]);
    let o = t.run(
        &a,
        ProtocolOp::ModifyRequest {
            dn: "cn=front end,cn=configuration".into(),
            changes: vec![m(ModOp::Replace, "ibm-slapdIdleTimeOut", &["600"])],
        },
        Vec::new(),
    );
    assert_eq!(code(&o), ResultCode::Success, "{:?}", o.replies[0].op);
    let r = t.search(&a, "cn=front end,cn=configuration", Scope::Base, "(objectClass=*)", &["ibm-slapdIdleTimeOut"]);
    assert_eq!(vals(&r, "ibm-slapdIdleTimeOut"), ["600"]);
    // Not in the root DSE's naming contexts, as on the primary.
    let r = t.search(&anon, "", Scope::Base, "(objectClass=*)", &["namingContexts"]);
    assert!(!vals(&r, "namingContexts").iter().any(|n| n.contains("CONFIGURATION")));
}

/// A backend that hides everything below its root from anonymous users,
/// as the projected backend does: searches succeed with no entries.
struct Hidden;

impl dirsrv::backend::Backend for Hidden {
    fn name(&self) -> &'static str {
        "hidden"
    }
    fn suffixes(&self) -> Vec<Dn> {
        vec![Dn::parse("os400-sys=SYSTEM").unwrap()]
    }
    fn read_only(&self) -> bool {
        true
    }
    fn resolve(&self, _: &mut Txn<'_>, schema: &Schema, _: &Session, dn: &Dn) -> dirsrv::ops::OpResult<dirsrv::backend::Resolution> {
        let root = Dn::parse("os400-sys=SYSTEM").unwrap();
        let id = if schema.dn_eq(dn, &root) { 1 } else { 2 };
        Ok(dirsrv::backend::Resolution {
            entry: Some(id),
            matched: Some(1),
        })
    }
    fn entry(&self, _: &mut Txn<'_>, _: &Schema, s: &Session, id: u64) -> dirsrv::ops::OpResult<Option<dirsrv::backend::DirEntry>> {
        Ok((id == 1 || s.dn.is_some()).then(|| dirsrv::backend::DirEntry {
            id,
            dn: if id == 1 { "OS400-SYS=SYSTEM".into() } else { "cn=accounts,OS400-SYS=SYSTEM".into() },
            attrs: vec![text("objectClass", &[if id == 1 { "os400-root" } else { "container" }])],
        }))
    }
    fn parent(&self, _: &mut Txn<'_>, id: u64) -> dirsrv::ops::OpResult<Option<u64>> {
        Ok((id == 2).then_some(1))
    }
    fn children(&self, _: &mut Txn<'_>, id: u64, _: usize) -> dirsrv::ops::OpResult<u64> {
        Ok(u64::from(id == 1))
    }
    fn scan(
        &self,
        _: &mut Txn<'_>,
        _: &Schema,
        s: &Session,
        base: u64,
        scope: Scope,
        _: &ldap::Filter,
        state: &mut dirsrv::backend::Scan,
        _: usize,
    ) -> dirsrv::ops::OpResult<Vec<u64>> {
        *state = dirsrv::backend::Scan::Done;
        let mut v = Vec::new();
        if scope != Scope::One {
            v.push(base);
        }
        if base == 1 && scope != Scope::Base && s.dn.is_some() {
            v.push(2);
        }
        Ok(v)
    }
}

#[test]
fn hidden_entries_answer_empty() {
    let mut t = T::new();
    t.dir.add_backend(Box::new(Hidden));
    let anon = Session::anonymous();
    let r = t.search(&anon, "os400-sys=SYSTEM", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(dns(&r), ["OS400-SYS=SYSTEM"]);
    let r = t.search(&anon, "os400-sys=SYSTEM", Scope::One, "(objectClass=*)", &[]);
    assert_eq!((done(&r).code, dns(&r).len()), (ResultCode::Success, 0));
    let r = t.search(&anon, "cn=accounts,os400-sys=SYSTEM", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!((done(&r).code, dns(&r).len()), (ResultCode::Success, 0));
    let r = t.search(&anon, "", Scope::Base, "(objectClass=*)", &["namingContexts"]);
    assert!(vals(&r, "namingContexts").contains(&"OS400-SYS=SYSTEM".to_string()));
    let o = t.run(
        &anon,
        ProtocolOp::CompareRequest {
            dn: "os400-sys=SYSTEM".into(),
            attr: "objectClass".into(),
            value: b"os400-root".to_vec(),
        },
        Vec::new(),
    );
    assert_eq!(code(&o), ResultCode::CompareTrue);
    let a = t.admin.clone();
    let o = t.add(&a, "cn=x,os400-sys=SYSTEM", &[text("objectClass", &["container"]), text("cn", &["x"])]);
    assert_eq!(code(&o), ResultCode::UnwillingToPerform);
}
