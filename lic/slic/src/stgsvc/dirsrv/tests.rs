// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Requests 1560-1567 over a store in memory and files in a map: who may
//! ask, binds of the administrator and of profiles, an operation and its
//! commit, searches in pages, a persistent search's changes,
//! `cn=configuration` written back, a job's end.

#![allow(clippy::many_single_char_names, clippy::match_same_arms)]

use std::collections::HashMap;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use dirsrv::config::{InstanceList, SlapdConf, conf_path, shipped};
use ldap::ResultCode;
use ldap::control::{ControlValue, PersistentSearch, change_type};
use ldap::filter::Filter;
use ldap::msg::{
    Attribute, BindAuth, DerefAliases, LdapMessage, ModOp, Modification, ProtocolOp, Scope,
    SearchRequest,
};
use qsname::Name;
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, usrprf};
use stg::JobInfo;
use stg::dir::{DirBudget, DirClose, DirConn, DirState};
use store::obj::{CreateParams, ObjectId, ops, types};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig};

use super::hooks::BindCtx;
use super::instance::Host;
use super::serve::State;

const NOW: u64 = 1_794_571_200;
const DIRDB_CONF: &str =
    "cn=Directory, cn=RDBM Backends, cn=IBM Directory, cn=Schemas, cn=Configuration";

fn sysval(n: &str) -> String {
    match n {
        "QSECURITY" => "40",
        "QPWDLVL" => "3",
        "QPWDMINLEN" => "6",
        "QPWDMAXLEN" => "128",
        "QPWDRULES" => "*PWDSYSVAL",
        "QPWDEXPITV" => "*NOMAX",
        "QMAXSIGN" => "3",
        "QMAXSGNACN" => "2",
        _ => "",
    }
    .into()
}

struct Mem {
    store: Store,
    qsys: ObjectId,
    files: HashMap<String, String>,
}

impl Host for Mem {
    fn read(&mut self, path: &str) -> Result<Option<String>, String> {
        Ok(self.files.get(path).cloned())
    }

    fn write(&mut self, path: &str, text: &str) -> Result<(), String> {
        self.files.insert(path.into(), text.into());
        Ok(())
    }

    fn instances(&mut self) -> Result<InstanceList, dirsrv::config::ConfigError> {
        InstanceList::parse(shipped::INSTANCES)
    }

    fn bind_ctx(&self) -> BindCtx {
        BindCtx {
            values: sec::signon::Values::from_sysvals(&sysval),
            now: NOW,
            qsecurity: 40,
        }
    }

    fn with_store<T>(&mut self, f: impl FnOnce(Txn<'_>, ObjectId) -> T) -> Option<T> {
        let t = self.store.begin().ok()?;
        Some(f(t, self.qsys))
    }
}

fn path() -> String {
    conf_path(dirsrv::config::ROOT, "QUSRDIR")
}

/// The shipped store with `QUSRDIRDB`, profile `BOB`, and `QUSRDIR`'s
/// file with an administrator password and the suffix `o=Acme`.
fn host() -> Mem {
    let d = Arc::new(MemBlockDev::with_seed(16384, 7));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: Name::parse("QUSRDIRDB").unwrap(),
            type_code: types::LIB,
            attribute: "PROD".into(),
            text: String::new(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: true,
        },
    )
    .unwrap();
    sec::profile::shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "BOB"), ("PASSWORD", "Right123")]),
    )
    .unwrap();
    t.commit().unwrap();
    store.asp_superblock_set_qsys(qsys);
    let mut conf = SlapdConf::parse(&path(), &shipped::slapd_conf("QUSRDIR", "SYSTEM")).unwrap();
    conf.set_admin(None, &dirsrv::pwd::hash(b"secret")).unwrap();
    conf.set(
        DIRDB_CONF,
        "ibm-slapdSuffix",
        &["cn=localhost", "cn=ibmpolicies", "o=Acme"],
    )
    .unwrap();
    let mut files = HashMap::new();
    files.insert(path(), conf.text().to_string());
    Mem { store, qsys, files }
}

fn job(name: &str, user: &str, number: u32) -> JobInfo {
    JobInfo {
        name: name.into(),
        user: user.into(),
        number,
        ..JobInfo::default()
    }
}

fn me() -> JobInfo {
    job("QUSRDIR", "QDIRSRV", 7)
}

fn conn() -> DirConn {
    DirConn {
        client: "10.0.0.9:50000".into(),
        ..DirConn::default()
    }
}

fn msg(id: i32, op: ProtocolOp) -> Vec<u8> {
    LdapMessage {
        id,
        op,
        controls: Vec::new(),
    }
    .encode()
}

fn bind_msg(id: i32, dn: &str, pw: &str) -> Vec<u8> {
    msg(
        id,
        ProtocolOp::BindRequest {
            version: 3,
            name: dn.into(),
            auth: BindAuth::Simple(pw.as_bytes().to_vec()),
        },
    )
}

fn search_msg(id: i32, base: &str, scope: Scope, filter: &str) -> SearchRequest {
    let _ = id;
    SearchRequest {
        base: base.into(),
        scope,
        deref: DerefAliases::Always,
        size_limit: 0,
        time_limit: 0,
        types_only: false,
        filter: Filter::parse(filter).unwrap(),
        attrs: vec!["1.1".into()],
    }
}

fn decoded(b: &[u8]) -> LdapMessage {
    LdapMessage::decode(b).unwrap()
}

fn code(b: &[u8]) -> ResultCode {
    decoded(b).op.result().unwrap().code
}

fn entries(v: &[Vec<u8>]) -> usize {
    v.iter()
        .filter(|b| matches!(decoded(b).op, ProtocolOp::SearchResultEntry { .. }))
        .count()
}

fn add(st: &mut State, h: &mut Mem, s: u64, id: i32, dn: &str) -> ResultCode {
    let cn = dn.split(',').next().unwrap().trim_start_matches("cn=");
    let a = st
        .op(
            h,
            &me(),
            s,
            &msg(
                id,
                ProtocolOp::AddRequest {
                    dn: dn.into(),
                    attrs: vec![
                        Attribute::new("objectClass", &["person"]),
                        Attribute::new("cn", &[cn]),
                        Attribute::new("sn", &["S"]),
                    ],
                },
            ),
        )
        .unwrap();
    code(&a.replies[0])
}

fn admin_session(st: &mut State, h: &mut Mem) -> u64 {
    let s = st.open(h, &me(), "QUSRDIR", conn()).unwrap().session;
    let a = st
        .bind(
            h,
            &me(),
            s,
            &bind_msg(1, "cn=Administrator", "secret"),
            conn(),
        )
        .unwrap();
    assert_eq!(code(&a.replies[0]), ResultCode::Success);
    assert_eq!(a.bound.as_deref(), Some("cn=Administrator"));
    s
}

#[test]
fn only_the_instance_job_asks() {
    let mut h = host();
    let mut st = State::default();
    for j in [job("QUSRDIR", "QSECOFR", 7), job("OTHER", "QDIRSRV", 7)] {
        let e = st.open(&mut h, &j, "QUSRDIR", conn()).unwrap_err();
        assert_eq!(e.msgid, "CPF2189");
    }
    let e = st
        .open(&mut h, &job("NOSUCH", "QDIRSRV", 7), "NOSUCH", conn())
        .unwrap_err();
    assert_eq!(e.msgid, "GLD0203");
    let o = st.open(&mut h, &me(), "qusrdir", conn()).unwrap();
    assert_eq!(o.settings.port, 389);
    assert_eq!(o.settings.secure_port, 636);
    assert_eq!(o.settings.app_id, "QS400_DIRECTORY_SERVER");
    // The file got its server ID.
    assert!(h.files[&path()].contains("ibm-slapdServerId: "));
    // Another job may not use the session.
    let e = st
        .bind(
            &mut h,
            &job("QUSRDIR", "QDIRSRV", 8),
            o.session,
            &bind_msg(1, "", ""),
            conn(),
        )
        .unwrap_err();
    assert_eq!(e.msgid, "CPF9898");
}

#[test]
fn operations_commit_and_searches_page() {
    let mut h = host();
    let mut st = State::default();
    let s = admin_session(&mut st, &mut h);
    let a = st
        .op(
            &mut h,
            &me(),
            s,
            &msg(
                2,
                ProtocolOp::AddRequest {
                    dn: "o=Acme".into(),
                    attrs: vec![
                        Attribute::new("objectClass", &["organization"]),
                        Attribute::new("o", &["Acme"]),
                    ],
                },
            ),
        )
        .unwrap();
    assert_eq!(code(&a.replies[0]), ResultCode::Success);
    assert_eq!(decoded(&a.replies[0]).id, 2);
    for i in 0..5 {
        assert_eq!(
            add(&mut st, &mut h, s, 3 + i, &format!("cn=P{i},o=Acme")),
            ResultCode::Success
        );
    }
    // A failed add leaves nothing.
    assert_eq!(
        add(&mut st, &mut h, s, 9, "cn=P0,o=Acme"),
        ResultCode::EntryAlreadyExists
    );
    // An anonymous session may not add.
    let anon = st.open(&mut h, &me(), "QUSRDIR", conn()).unwrap().session;
    assert_eq!(
        add(&mut st, &mut h, anon, 10, "cn=Q,o=Acme"),
        ResultCode::InsufficientAccessRights
    );
    let b = DirBudget {
        entries: 2,
        bytes: 1 << 20,
    };
    let req = msg(
        11,
        ProtocolOp::SearchRequest(search_msg(11, "o=Acme", Scope::One, "(cn=*)")),
    );
    let mut a = st.search(&mut h, &me(), s, &req, b).unwrap();
    let mut n = entries(&a.replies);
    let mut turns = 1;
    while let DirState::More { cursor } = a.state {
        a = st.next(&mut h, &me(), s, cursor, b).unwrap();
        n += entries(&a.replies);
        turns += 1;
    }
    assert_eq!(n, 5);
    assert!(turns >= 3);
    let last = decoded(a.replies.last().unwrap());
    assert_eq!(last.id, 11);
    assert!(matches!(last.op, ProtocolOp::SearchResultDone(_)));
}

#[test]
fn persistent_searches_see_changes() {
    let mut h = host();
    let mut st = State::default();
    let s = admin_session(&mut st, &mut h);
    let a = st
        .op(
            &mut h,
            &me(),
            s,
            &msg(
                2,
                ProtocolOp::AddRequest {
                    dn: "o=Acme".into(),
                    attrs: vec![
                        Attribute::new("objectClass", &["organization"]),
                        Attribute::new("o", &["Acme"]),
                    ],
                },
            ),
        )
        .unwrap();
    assert_eq!(code(&a.replies[0]), ResultCode::Success);
    let ps = PersistentSearch {
        change_types: change_type::ADD,
        changes_only: true,
        return_ecs: false,
    };
    let req = LdapMessage {
        id: 5,
        op: ProtocolOp::SearchRequest(search_msg(5, "o=Acme", Scope::Sub, "(objectClass=*)")),
        controls: vec![ps.to_control(true)],
    }
    .encode();
    let a = st
        .search(&mut h, &me(), s, &req, DirBudget::default())
        .unwrap();
    assert!(
        matches!(a.state, DirState::Watching { .. }),
        "{:?}",
        a.state
    );
    let other = admin_session(&mut st, &mut h);
    assert_eq!(
        add(&mut st, &mut h, other, 3, "cn=New,o=Acme"),
        ResultCode::Success
    );
    let c = st.changes(&me(), s, 10).unwrap();
    assert_eq!(c.replies.len(), 1);
    assert_eq!(decoded(&c.replies[0]).id, 5);
    st.close(&me(), s, DirClose::Message { id: 5 }).unwrap();
    assert_eq!(
        add(&mut st, &mut h, other, 4, "cn=New2,o=Acme"),
        ResultCode::Success
    );
    assert!(st.changes(&me(), s, 10).unwrap().replies.is_empty());
}

#[test]
fn profiles_bind_as_sign_on_does() {
    let mut h = host();
    let mut st = State::default();
    let s = st.open(&mut h, &me(), "QUSRDIR", conn()).unwrap().session;
    let dn = "os400-profile=BOB,cn=accounts,os400-sys=SYSTEM";
    let a = st
        .bind(&mut h, &me(), s, &bind_msg(1, dn, "Right123"), conn())
        .unwrap();
    assert_eq!(code(&a.replies[0]), ResultCode::Success);
    assert_eq!(a.bound.as_deref(), Some(dn));
    // Bound as BOB, the profile is seen.
    let req = msg(
        2,
        ProtocolOp::SearchRequest(search_msg(2, dn, Scope::Base, "(objectClass=*)")),
    );
    let r = st
        .search(&mut h, &me(), s, &req, DirBudget::default())
        .unwrap();
    assert_eq!(entries(&r.replies), 1);
    // Wrong passwords count toward QMAXSIGN (3), then the profile is
    // disabled (QMAXSGNACN 2) and the right one fails too.
    for i in 0..3 {
        let a = st
            .bind(&mut h, &me(), s, &bind_msg(3 + i, dn, "wrong"), conn())
            .unwrap();
        assert_eq!(code(&a.replies[0]), ResultCode::InvalidCredentials);
        assert_eq!(a.bound, None);
    }
    let a = st
        .bind(&mut h, &me(), s, &bind_msg(9, dn, "Right123"), conn())
        .unwrap();
    assert_eq!(code(&a.replies[0]), ResultCode::InvalidCredentials);
    let disabled = h
        .with_store(|mut t, q| {
            sec::profile::objects::read(&mut t, q, "BOB")
                .unwrap()
                .1
                .disabled
        })
        .unwrap();
    assert!(disabled);
}

#[test]
fn configuration_changes_are_written() {
    let mut h = host();
    let mut st = State::default();
    let s = admin_session(&mut st, &mut h);
    let m = msg(
        2,
        ProtocolOp::ModifyRequest {
            dn: "cn=Configuration".into(),
            changes: vec![Modification {
                op: ModOp::Replace,
                attr: Attribute::new("ibm-slapdSizeLimit", &["42"]),
            }],
        },
    );
    let a = st.op(&mut h, &me(), s, &m).unwrap();
    assert_eq!(code(&a.replies[0]), ResultCode::Success);
    assert!(h.files[&path()].contains("ibm-slapdSizeLimit: 42"));
    assert_eq!(
        st.with_instance("QUSRDIR", |i| i.dir.settings.size_limit),
        Some(42)
    );
    // An attribute QS/400 does not take is refused, the file unchanged.
    let m = msg(
        3,
        ProtocolOp::ModifyRequest {
            dn: "cn=Configuration".into(),
            changes: vec![Modification {
                op: ModOp::Replace,
                attr: Attribute::new("ibm-slapdPwEncryption", &["aes256"]),
            }],
        },
    );
    let a = st.op(&mut h, &me(), s, &m).unwrap();
    assert_eq!(code(&a.replies[0]), ResultCode::UnwillingToPerform);
    assert!(!h.files[&path()].contains("aes256"));
}

#[test]
fn a_jobs_end_drops_its_sessions() {
    let mut h = host();
    let mut st = State::default();
    let s = admin_session(&mut st, &mut h);
    st.job_ended(7);
    assert!(
        st.bind(&mut h, &me(), s, &bind_msg(1, "", ""), conn())
            .is_err()
    );
    assert!(st.with_instance("QUSRDIR", |_| ()).is_none());
}
