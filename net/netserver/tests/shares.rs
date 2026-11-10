// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NetServer` end to end: SMB clients (the library's own test helpers)
//! against `NetServer` jobs over one IFS in memory, through handed-over
//! loopback sockets as `QZLSSERVER` hands them.

#![allow(clippy::many_single_char_names, clippy::cast_possible_truncation)]

mod smbc;

use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hmac::{Hmac, Mac};
use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::ops::audit::MemAudit;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_RDWR};
use ifs::ops::{Attr, Config as IfsConfig, Ctx, Ifs, OpenArgs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData};
use md4::{Digest, Md4};
use netserver::ifs::TextConversion;
use netserver::print::{OutQueue, PrintError, PrintJob, Printer};
use netserver::signon::{Refusal, SignOn, SignOnRequest, SignedOn};
use netserver::{FileShare, NetServer, NetServerConfig, PrintShareCfg, nbss};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;
use smb_server::ntstatus::*;
use smb_server::wire::header::Command;
use smbc::auth::{LoginOptions, login_with, request, status};
use smbc::client::*;
use smbc::durable::{SignIn, contexts, dhnc, dhnq, lease_v1, reset};
use smbc::lease::{OPLOCK_LEVEL_LEASE, RWH};
use smbc::{negotiate_dialects_from, tree_connect_body};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

type HmacMd5 = Hmac<md5::Md5>;

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let mut m = HmacMd5::new_from_slice(key).unwrap();
    for p in parts {
        m.update(p);
    }
    m.finalize().into_bytes().into()
}

fn principal(user: &str) -> Principal {
    let mut p = Principal {
        user: user.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if user == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0 | SpecialAuth::SECADM.0;
    }
    p
}

/// One IFS in memory: `/home/ALICE` owned by `ALICE`, `/pub` for
/// everyone, `/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR`.
struct Fs {
    ifs: Arc<Ifs<MemBackend>>,
    env: Arc<MemEnv>,
    audit: Arc<MemAudit>,
    owner: AtomicU64,
}

impl Fs {
    fn new() -> Arc<Self> {
        let q = MemQsys::new(Auth::USE);
        q.add_lib("QGPL");
        q.add_obj("QGPL", "QCLSRC", "*FILE", "PF-SRC", "QSYS");
        let rec = |l: &str| ccsid::to_ebcdic(&format!("{l:<80}")).unwrap_or_default();
        q.add_member(
            "QGPL",
            "QCLSRC",
            "HELLO",
            MemberData {
                records: vec![rec("PGM"), rec("ENDPGM")],
                record_len: 80,
                source: true,
                ccsid: 37,
            },
        );
        let env = MemEnv::default()
            .user("QSYS", 0)
            .user("QSECOFR", 1)
            .user("QDFTOWN", 3)
            .user("ALICE", 110)
            .user("BOB", 120);
        let ifs = Ifs::new(MemBackend::new(), Box::new(q), IfsConfig::default());
        ifs.ensure_layout(&env).unwrap();
        let audit = MemAudit::default();
        let sec = principal("QSECOFR");
        let cx = Ctx {
            owner: Owner(u64::MAX),
            principal: &sec,
            env: &env,
            audit: &audit,
            job_ccsid: 37,
        };
        ifs.mkdir(&cx, "/home/ALICE", 0o755).unwrap();
        ifs.chgown(&cx, "/home/ALICE", "ALICE", true).unwrap();
        ifs.mkdir(&cx, "/pub", 0o777).unwrap();
        ifs.chmod(&cx, "/pub", 0o777).unwrap();
        ifs.release_owner(Owner(u64::MAX));
        Arc::new(Self {
            ifs: Arc::new(ifs),
            env: Arc::new(env),
            audit: Arc::new(audit),
            owner: AtomicU64::new(1),
        })
    }

    fn client(&self, user: &str) -> LocalIfs<MemBackend> {
        self.client_as(user, self.owner.fetch_add(1, Ordering::SeqCst))
    }

    /// A client as `owner`: a job's second link has its first one's.
    fn client_as(&self, user: &str, owner: u64) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: self.ifs.clone(),
            env: self.env.clone(),
            audit: self.audit.clone(),
            principal: principal(user),
            owner: Owner(owner),
            job_ccsid: 37,
        }
    }
}

/// Knows ALICE and BOB with the password `password`.
struct Profiles;

impl SignOn for Profiles {
    fn sign_on(&self, r: &SignOnRequest) -> Result<SignedOn, Refusal> {
        let profile = r.user.to_uppercase();
        if !matches!(profile.as_str(), "ALICE" | "BOB") {
            return Err(Refusal::NoSuchUser);
        }
        if r.nt_response.len() < 16 {
            return Err(Refusal::NotNtlmV2);
        }
        let nt: [u8; 16] = Md4::digest(utf16("password")).into();
        let key = hmac(&nt, &[&utf16(&format!("{profile}{}", r.domain))]);
        let (proof, blob) = r.nt_response.split_at(16);
        let computed = hmac(&key, &[&r.server_challenge, blob]);
        if proof != computed {
            return Err(Refusal::WrongPassword {
                disabled_now: false,
            });
        }
        Ok(SignedOn {
            profile,
            guest: false,
            session_base_key: Some(hmac(&key, &[&computed])),
        })
    }
}

/// What was printed: user, queue, document, data.
type Printed = Arc<Mutex<Vec<(String, String, String, Vec<u8>)>>>;

struct Spool(Printed);
struct SpoolJob(Printed, (String, String, String), Vec<u8>);

impl Printer for Spool {
    fn open(&self, user: &str, q: &OutQueue, doc: &str) -> Result<Box<dyn PrintJob>, PrintError> {
        Ok(Box::new(SpoolJob(
            self.0.clone(),
            (user.into(), format!("{}/{}", q.library, q.name), doc.into()),
            Vec::new(),
        )))
    }
}

impl PrintJob for SpoolJob {
    fn write(&mut self, data: &[u8]) -> Result<(), PrintError> {
        self.2.extend_from_slice(data);
        Ok(())
    }
    fn close(self: Box<Self>) -> Result<(), PrintError> {
        let (u, q, d) = self.1;
        self.0.lock().unwrap().push((u, q, d, self.2));
        Ok(())
    }
}

fn config() -> NetServerConfig {
    let mut ro = FileShare::new("PUBRO", "/pub");
    ro.read_only = true;
    let mut cnv = FileShare::new("PUBCNV", "/pub");
    cnv.conversion = Some(TextConversion::new(819, vec!["TXT".into()]));
    let mut qcnv = FileShare::new("QSYSCNV", "/QSYS.LIB");
    qcnv.conversion = Some(TextConversion::new(0, vec!["MBR".into()]));
    NetServerConfig {
        server_name: "QSTEST".into(),
        file_shares: vec![
            FileShare::new("HOME", "/home/ALICE"),
            FileShare::new("PUB", "/pub"),
            ro,
            FileShare::new("QSYS", "/QSYS.LIB"),
            cnv,
            qcnv,
        ],
        print_shares: vec![PrintShareCfg {
            name: "PRT01".into(),
            text: "Printer".into(),
            queue: OutQueue {
                library: "QGPL".into(),
                name: "PRT01".into(),
            },
            driver: "Microsoft PCL6 Class Driver".into(),
        }],
        durable_timeout: Duration::from_millis(500),
        ..NetServerConfig::default()
    }
}

/// A job: a `NetServer` whose IFS client runs as `user`, serving every
/// connection accepted on its own port.
struct Job {
    server: Arc<NetServer>,
    addr: SocketAddr,
    printed: Printed,
}

fn job(fs: &Arc<Fs>, user: &str) -> Job {
    job_as(fs, user, false)
}

/// A job whose leases are in the IFS's lease table when `leases`.
fn job_as(fs: &Arc<Fs>, user: &str, leases: bool) -> Job {
    let printed: Printed = Arc::default();
    let owner = fs.owner.fetch_add(1, Ordering::SeqCst);
    let f = fs.clone();
    let u = user.to_owned();
    let server = NetServer::build(
        &config(),
        Box::new(move || Box::new(f.client_as(&u, owner)) as Box<dyn IfsClient>),
        Arc::new(Profiles),
        &(Arc::new(Spool(printed.clone())) as Arc<dyn Printer>),
    )
    .unwrap();
    if leases {
        let (f, u) = (fs.clone(), user.to_owned());
        server
            .start_leases(Box::new(move || {
                Box::new(f.client_as(&u, owner)) as Box<dyn IfsClient>
            }))
            .unwrap();
    }
    let server = Arc::new(server);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let s = server.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let s = s.clone();
            std::thread::spawn(move || {
                let _ = s.serve_one(stream);
            });
        }
    });
    Job {
        server,
        addr,
        printed,
    }
}

async fn alice(addr: SocketAddr, share: &str) -> Client {
    smbc::durable::user_client(
        addr,
        SignIn {
            share,
            user: "alice",
            ..SignIn::default()
        },
    )
    .await
}

fn mk(name: &str) -> Create<'_> {
    Create {
        name,
        disposition: FILE_OPEN_IF,
        ..Create::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn files_round_trip_through_the_ifs() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut c = alice(j.addr, "HOME").await;
    let id = c
        .create(Create {
            disposition: FILE_CREATE,
            ..mk("Notes.txt")
        })
        .await
        .unwrap()
        .file_id;
    assert_eq!(c.write(id, 0, b"hello, world").await.unwrap(), 12);
    c.close(id).await;
    // The IFS holds it, owned by the profile.
    let me = fs.client("ALICE");
    let st = me.stat("/home/ALICE/Notes.txt", true).unwrap();
    assert_eq!(st.size, 12);
    assert_eq!(st.owner, "ALICE");
    // Any case opens it; the listing keeps the case it was made with.
    let id = c
        .open("NOTES.TXT", FILE_GENERIC_READ, SHARE_ALL)
        .await
        .unwrap();
    assert_eq!(c.read(id, 7, 5).await.unwrap(), b"world");
    c.close(id).await;
    let dir = c.open_dir("").await;
    assert_eq!(c.list(dir, 0, "*").await.unwrap(), [".", "..", "Notes.txt"]);
    assert_eq!(c.list(dir, 0x01, "*.TXT").await.unwrap(), ["Notes.txt"]);
    c.close(dir).await;
    // A directory, a rename into it, a delete.
    let d = c
        .create(Create {
            access: FILE_GENERIC_READ,
            options: FILE_DIRECTORY_FILE,
            disposition: FILE_CREATE,
            ..mk("docs")
        })
        .await
        .unwrap()
        .file_id;
    c.close(d).await;
    let id = c
        .open("Notes.txt", DELETE | FILE_GENERIC_READ, SHARE_ALL)
        .await
        .unwrap();
    let mut rename = vec![0u8; 20];
    let target = smbc::utf16le("docs\\N2.txt");
    rename[16..20].copy_from_slice(&u32::try_from(target.len()).unwrap().to_le_bytes());
    rename.extend_from_slice(&target);
    assert_eq!(
        c.set_info(id, FILE_RENAME_INFORMATION, rename).await,
        STATUS_SUCCESS
    );
    c.close(id).await;
    assert!(me.stat("/home/ALICE/docs/N2.txt", true).is_ok());
    let id = c.open("docs\\n2.TXT", DELETE, SHARE_ALL).await.unwrap();
    assert_eq!(c.set_delete(id, true).await, STATUS_SUCCESS);
    c.close(id).await;
    assert!(me.stat("/home/ALICE/docs/N2.txt", true).is_err());
    assert_eq!(
        c.open("nothere\\x", FILE_GENERIC_READ, SHARE_ALL)
            .await
            .err(),
        Some(STATUS_OBJECT_PATH_NOT_FOUND)
    );
    me.release();
}

#[tokio::test(flavor = "multi_thread")]
async fn dos_attributes_are_the_ifs_attributes() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut c = alice(j.addr, "HOME").await;
    let id = c.create(mk("a.txt")).await.unwrap().file_id;
    c.close(id).await;
    let me = fs.client("ALICE");
    me.set_attr("/home/ALICE/a.txt", &Attr::PcHidden(true), true)
        .unwrap();
    let id = c
        .open(
            "a.txt",
            FILE_GENERIC_READ | FILE_WRITE_ATTRIBUTES,
            SHARE_ALL,
        )
        .await
        .unwrap();
    let basic = c.query_file_info(id, FILE_BASIC_INFORMATION).await.unwrap();
    let attrs = u32_at(&basic, 32);
    assert_eq!(attrs & 0x2, 0x2, "hidden");
    // Read-only and archive set through SMB land in the IFS.
    let mut set = basic.clone();
    set[32..36].copy_from_slice(&(0x1u32 | 0x20).to_le_bytes());
    assert_eq!(
        c.set_info(id, FILE_BASIC_INFORMATION, set).await,
        STATUS_SUCCESS
    );
    c.close(id).await;
    let got = me
        .get_attrs(
            "/home/ALICE/a.txt",
            &[
                ifs::ops::AttrId::PcReadOnly,
                ifs::ops::AttrId::PcHidden,
                ifs::ops::AttrId::PcArchive,
            ],
            true,
        )
        .unwrap();
    assert!(
        got.contains(&Attr::PcReadOnly(true))
            && got.contains(&Attr::PcArchive(true))
            && got.contains(&Attr::PcHidden(false)),
        "{got:?}"
    );
    // The file ID is the IFS's, the same in listings and opens.
    let id = c
        .open("a.txt", FILE_READ_ATTRIBUTES, SHARE_ALL)
        .await
        .unwrap();
    let internal = u64_at(
        &c.query_file_info(id, FILE_INTERNAL_INFORMATION)
            .await
            .unwrap(),
        0,
    );
    c.close(id).await;
    let dir = c.open_dir("").await;
    let (_, buf) = c
        .query_directory(dir, FILE_ID_BOTH_DIRECTORY_INFORMATION, 0, "a.txt", 65536)
        .await;
    assert_eq!(smbc::id_both_entries(&buf)[0].1, internal);
    me.release();
}

#[tokio::test(flavor = "multi_thread")]
async fn share_modes_and_locks_reach_other_jobs() {
    let fs = Fs::new();
    let (a, b) = (job(&fs, "ALICE"), job(&fs, "ALICE"));
    let mut c1 = alice(a.addr, "PUB").await;
    let mut c2 = alice(b.addr, "PUB").await;
    let id = c1
        .create(Create {
            share: SHARE_READ,
            ..mk("shared.dat")
        })
        .await
        .unwrap()
        .file_id;
    c1.write(id, 0, &[0; 100]).await.unwrap();
    // The other job may read, not write.
    assert_eq!(
        c2.open("shared.dat", FILE_GENERIC_WRITE, SHARE_ALL)
            .await
            .err(),
        Some(STATUS_SHARING_VIOLATION)
    );
    // A reader that denies writing conflicts with the writer.
    let r = c2.open("shared.dat", FILE_GENERIC_READ, SHARE_READ).await;
    assert_eq!(r.err(), Some(STATUS_SHARING_VIOLATION));
    let r = c2
        .open("shared.dat", FILE_GENERIC_READ, SHARE_ALL)
        .await
        .unwrap();
    // A byte-range lock in one job refuses the other's.
    assert_eq!(
        c1.lock(id, &[(0, 10, LOCK_EXCLUSIVE | LOCK_FAIL_IMMEDIATELY)])
            .await,
        STATUS_SUCCESS
    );
    assert_eq!(
        c2.lock(r, &[(5, 10, LOCK_SHARED | LOCK_FAIL_IMMEDIATELY)])
            .await,
        STATUS_LOCK_NOT_GRANTED
    );
    assert_eq!(c1.lock(id, &[(0, 10, LOCK_UNLOCK)]).await, STATUS_SUCCESS);
    assert_eq!(
        c2.lock(r, &[(5, 10, LOCK_SHARED | LOCK_FAIL_IMMEDIATELY)])
            .await,
        STATUS_SUCCESS
    );
    c1.close(id).await;
    c2.close(r).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_read_only_share_and_the_ifs_authority_both_hold() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut c = alice(j.addr, "PUBRO").await;
    assert_eq!(c.create(mk("x")).await.err(), Some(STATUS_ACCESS_DENIED));
    // BOB's job reaches ALICE's home only as the IFS lets BOB.
    let b = job(&fs, "BOB");
    let mut c = smbc::durable::user_client(
        b.addr,
        SignIn {
            share: "HOME",
            user: "bob",
            client_guid: [0xB2; 16],
            ..SignIn::default()
        },
    )
    .await;
    assert_eq!(
        c.create(mk("bobs.txt")).await.err(),
        Some(STATUS_ACCESS_DENIED)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sign_on_failures_and_one_profile_per_job() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut s = TcpStream::connect(j.addr).await.unwrap();
    negotiate_dialects_from(&mut s, &[0x0210], [1; 16]).await;
    let l = login_with(&mut s, 1, 0, "alice", "wrong", LoginOptions::default()).await;
    assert_eq!(l.status, STATUS_LOGON_FAILURE);
    let _c = alice(j.addr, "HOME").await;
    assert_eq!(j.server.profile().as_deref(), Some("ALICE"));
    let mut s = TcpStream::connect(j.addr).await.unwrap();
    negotiate_dialects_from(&mut s, &[0x0210], [2; 16]).await;
    let l = login_with(&mut s, 1, 0, "bob", "password", LoginOptions::default()).await;
    assert_eq!(
        l.status, STATUS_LOGON_FAILURE,
        "a second profile in the job"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_print_share_spools_as_the_profile() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut s = smbc::durable::negotiated(j.addr, 0x0210, [0xA1; 16]).await;
    let l = login_with(&mut s, 1, 0, "alice", "password", LoginOptions::default()).await;
    let t = request(
        &mut s,
        Command::TreeConnect,
        3,
        l.session_id,
        0,
        &tree_connect_body(r"\\QSTEST\PRT01"),
    )
    .await;
    assert_eq!(status(&t), STATUS_SUCCESS);
    let (h, body) = smbc::parse_response_header(&t);
    assert_eq!(
        smb_server::wire::messages::TreeConnectResponse::parse(body)
            .unwrap()
            .share_type,
        3
    );
    let mut c = Client::from_parts(s, l.session_id, h.tree_id().unwrap());
    let id = c
        .create(Create {
            access: FILE_GENERIC_WRITE,
            disposition: FILE_OVERWRITE_IF,
            ..mk("Report")
        })
        .await
        .unwrap()
        .file_id;
    c.write(id, 0, b"page one\x0c").await.unwrap();
    c.write(id, 9, b"page two").await.unwrap();
    c.close(id).await;
    let printed = j.printed.lock().unwrap().clone();
    assert_eq!(printed.len(), 1);
    let (u, q, _, data) = &printed[0];
    assert_eq!((u.as_str(), q.as_str()), ("ALICE", "QGPL/PRT01"));
    assert_eq!(data, b"page one\x0cpage two");
}

#[tokio::test(flavor = "multi_thread")]
async fn qsys_lib_lists_through_the_ifs() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut c = alice(j.addr, "QSYS").await;
    let dir = c.open_dir("QGPL.LIB\\QCLSRC.FILE").await;
    assert_eq!(c.list(dir, 0, "*").await.unwrap(), [".", "..", "HELLO.MBR"]);
    c.close(dir).await;
    let id = c
        .open(
            "qgpl.lib\\qclsrc.file\\hello.mbr",
            FILE_GENERIC_READ,
            SHARE_ALL,
        )
        .await
        .unwrap();
    let data = c.read(id, 0, 4096).await.unwrap();
    assert!(!data.is_empty());
    c.close(id).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_routed_reconnect_takes_its_durable_open_back() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut c = alice(j.addr, "PUB").await;
    let leased = |name| Create {
        oplock: OPLOCK_LEVEL_LEASE,
        contexts: contexts(&[lease_v1([5; 16], RWH), dhnq()]),
        ..mk(name)
    };
    let resp = c.create(leased("durable.txt")).await.unwrap();
    assert_eq!(j.server.client_guids(), vec![[0xA1; 16]]);
    reset(c);
    for _ in 0..200 {
        if j.server.disconnected_opens() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(j.server.disconnected_opens(), 1);
    // The same client on a new connection to the same job.
    let mut c = alice(j.addr, "PUB").await;
    let back = c
        .create(Create {
            contexts: contexts(&[lease_v1([5; 16], RWH), dhnc(resp.file_id)]),
            oplock: OPLOCK_LEVEL_LEASE,
            ..mk("durable.txt")
        })
        .await;
    assert!(back.is_ok(), "{back:?}");
    drop(c);
    let server = j.server.clone();
    tokio::task::spawn_blocking(move || server.idle_until_released())
        .await
        .unwrap();
    assert_eq!(j.server.connections(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn port_139_answers_the_session_request_then_serves() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let server = j.server.clone();
    std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        nbss::answer(&mut s, "QSTEST", Duration::from_secs(5)).unwrap();
        let _ = server.serve_one(s);
    });
    let mut s = TcpStream::connect(addr).await.unwrap();
    let req = nbss::session_request(
        &nbss::NbName::new("QSTEST", nbss::SUFFIX_SERVER),
        &nbss::NbName::new("PC", 0),
    );
    s.write_all(&req).await.unwrap();
    let mut resp = [0u8; 4];
    s.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, nbss::positive_response());
    negotiate_dialects_from(&mut s, &[0x0210], [9; 16]).await;
    s.write_all(&[nbss::KEEP_ALIVE, 0, 0, 0]).await.unwrap();
    let l = login_with(&mut s, 1, 0, "alice", "password", LoginOptions::default()).await;
    assert_eq!(l.status, STATUS_SUCCESS);
    let t = request(
        &mut s,
        Command::TreeConnect,
        3,
        l.session_id,
        0,
        &tree_connect_body(r"\\QSTEST\PUB"),
    )
    .await;
    assert_eq!(status(&t), STATUS_SUCCESS);
}

/// NDR as a client marshals it, for the spoolss requests.
#[derive(Default)]
struct Stub(Vec<u8>, u32);

impl Stub {
    fn u32(&mut self, v: u32) -> &mut Self {
        self.0.resize(self.0.len().next_multiple_of(4), 0);
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn ptr(&mut self, present: bool) -> &mut Self {
        self.1 += 4;
        self.u32(if present { 0x0002_0000 + self.1 } else { 0 })
    }
    fn string(&mut self, text: &str) -> &mut Self {
        let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
        self.u32(units.len() as u32).u32(0).u32(units.len() as u32);
        for u in units {
            self.0.extend_from_slice(&u.to_le_bytes());
        }
        self
    }
    fn raw(&mut self, b: &[u8]) -> &mut Self {
        self.0.resize(self.0.len().next_multiple_of(4), 0);
        self.0.extend_from_slice(b);
        self
    }
    fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.u32(b.len() as u32);
        self.0.extend_from_slice(b);
        self
    }
    fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

const SPOOLSS: [u8; 20] = [
    0x78, 0x56, 0x34, 0x12, 0x34, 0x12, 0xcd, 0xab, 0xef, 0x00, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
    1, 0, 0, 0,
];

fn dword(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// The string an INFO buffer's offset at `at` (from `base`) points to.
fn info_string(b: &[u8], base: usize, at: usize) -> String {
    let from = base + dword(b, base + at) as usize;
    let units: Vec<u16> = b[from..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16(&units).unwrap()
}

/// alice on spoolss, bound, with `PRT01` open.
async fn spoolss_printer(
    addr: SocketAddr,
) -> (Client, smb_server::wire::messages::FileId, Vec<u8>) {
    use smbc::rpc::*;
    let mut c = ipc_client(addr, 0x0210, Some("alice")).await;
    let pipe = c.open_pipe("spoolss").await.expect("spoolss").file_id;
    assert_eq!(
        c.pipe_write(pipe, &bind_pdu(1, 4280, &[(0, SPOOLSS, NDR)]))
            .await,
        STATUS_SUCCESS
    );
    let (_, ack) = c.pipe_read(pipe, 4280).await;
    assert_eq!(ack_results(&ack), [(0, 0)]);
    let mut s = Stub::default();
    s.ptr(true)
        .string(r"\\QSTEST\PRT01")
        .ptr(false)
        .u32(0)
        .ptr(false)
        .u32(0x8)
        .u32(1)
        .u32(1)
        .ptr(false);
    let out = c.call(pipe, 2, 69, &s.take()).await.unwrap();
    assert_eq!(dword(&out, 20), 0, "OpenPrinterEx");
    (c, pipe, out[..20].to_vec())
}

/// The INFO buffer of a GetPrinter-style answer.
fn info_buffer(out: &[u8]) -> Vec<u8> {
    assert_ne!(dword(out, 0), 0);
    let len = dword(out, 4) as usize;
    out[8..8 + len].to_vec()
}

#[tokio::test(flavor = "multi_thread")]
async fn spoolss_prints_to_the_share_s_queue_as_create_does() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let (mut c, pipe, h) = spoolss_printer(j.addr).await;
    // GetPrinter 2: the driver and the output queue as the port.
    let mut s = Stub::default();
    s.raw(&h).u32(2).ptr(true).bytes(&[0; 4096]).u32(4096);
    let out = c.call(pipe, 3, 8, &s.take()).await.unwrap();
    let info = info_buffer(&out);
    assert_eq!(info_string(&info, 0, 8), "PRT01");
    assert_eq!(info_string(&info, 0, 12), "QGPL/PRT01");
    assert_eq!(info_string(&info, 0, 16), "Microsoft PCL6 Class Driver");
    // StartDoc, WritePrinter, EndDoc.
    let mut s = Stub::default();
    s.raw(&h)
        .u32(1)
        .u32(1)
        .ptr(true)
        .ptr(true)
        .ptr(false)
        .ptr(false)
        .string("Test Page");
    let out = c.call(pipe, 4, 17, &s.take()).await.unwrap();
    assert_eq!(dword(&out, 4), 0, "StartDocPrinter");
    let mut s = Stub::default();
    s.raw(&h).bytes(b"\x1bE test page\x0c").u32(13);
    let out = c.call(pipe, 5, 19, &s.take()).await.unwrap();
    assert_eq!((dword(&out, 0), dword(&out, 4)), (13, 0));
    let mut s = Stub::default();
    s.raw(&h);
    assert_eq!(c.call(pipe, 6, 23, &s.take()).await.unwrap(), [0; 4]);
    let printed = j.printed.lock().unwrap().clone();
    assert_eq!(
        printed,
        [(
            "ALICE".into(),
            "QGPL/PRT01".into(),
            "Test Page".into(),
            b"\x1bE test page\x0c".to_vec()
        )]
    );
}

/// Writes `files` under the driver store as QSECOFR.
fn driver_store(fs: &Fs, files: &[(&str, &[u8])]) {
    use ifs::ops::OpenArgs;
    use ifs::ops::flags::{O_CREAT, O_WRONLY};
    let c = fs.client("QSECOFR");
    for (path, data) in files {
        let full = format!("{}/{path}", netserver::print::drivers::DRIVER_STORE);
        let mut dir = String::new();
        for part in full
            .split('/')
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .split_last()
            .unwrap()
            .1
        {
            dir = format!("{dir}/{part}");
            let _ = c.mkdir(&dir, 0o755);
        }
        let fd = c
            .open(&full, OpenArgs::new(O_CREAT | O_WRONLY).mode(0o644))
            .unwrap();
        c.write(fd, data).unwrap();
        c.close(fd).unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn print_dollar_serves_the_driver_files_spoolss_names() {
    let fs = Fs::new();
    driver_store(
        &fs,
        &[
            (
                "x64/3/Microsoft PCL6 Class Driver.qsdrv",
                b"driver=unidrv.dll\ndata=pcl6.gpd\nconfig=unidrvui.dll\nfile=stdnames.gpd\n",
            ),
            ("x64/3/unidrv.dll", b"MZ driver"),
        ],
    );
    let j = job(&fs, "ALICE");
    let (mut c, pipe, h) = spoolss_printer(j.addr).await;
    let driver = |env: &str| {
        let mut s = Stub::default();
        s.raw(&h)
            .ptr(true)
            .string(env)
            .u32(3)
            .ptr(true)
            .bytes(&[0; 4096])
            .u32(4096)
            .u32(3)
            .u32(0);
        s.take()
    };
    let out = c.call(pipe, 3, 53, &driver("Windows x64")).await.unwrap();
    let info = info_buffer(&out);
    assert_eq!(
        info_string(&info, 0, 12),
        r"\\QSTEST\print$\x64\3\unidrv.dll"
    );
    assert_eq!(
        info_string(&info, 0, 28),
        r"\\QSTEST\print$\x64\3\stdnames.gpd"
    );
    // No x86 files: that environment has no driver.
    let out = c
        .call(pipe, 4, 53, &driver("Windows NT x86"))
        .await
        .unwrap();
    assert_eq!(
        dword(&out, out.len() - 4),
        1797,
        "ERROR_UNKNOWN_PRINTER_DRIVER"
    );

    // print$ is read-only.
    let mut p = alice(j.addr, "print$").await;
    let id = p
        .open(r"x64\3\unidrv.dll", FILE_GENERIC_READ, SHARE_ALL)
        .await
        .unwrap();
    assert_eq!(p.read(id, 0, 64).await.unwrap(), b"MZ driver");
    p.close(id).await;
    let made = p
        .create(Create {
            access: FILE_GENERIC_WRITE,
            disposition: FILE_CREATE,
            ..mk(r"x64\3\new.dll")
        })
        .await;
    assert!(made.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_driver_store_there_is_no_print_dollar() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut s = smbc::durable::negotiated(j.addr, 0x0210, [0xA3; 16]).await;
    let l = login_with(&mut s, 1, 0, "alice", "password", LoginOptions::default()).await;
    let t = request(
        &mut s,
        Command::TreeConnect,
        3,
        l.session_id,
        0,
        &tree_connect_body(r"\\QSTEST\print$"),
    )
    .await;
    assert_eq!(status(&t), STATUS_BAD_NETWORK_NAME);
}

const BREAK_WAIT: Duration = Duration::from_secs(10);

/// A lease on `name` with key `key` in state `state` through `c`.
async fn leased_open(
    c: &mut Client,
    name: &str,
    key: [u8; 16],
    state: u32,
    durable: bool,
) -> smb_server::wire::messages::CreateResponse {
    let mut list = vec![lease_v1(key, state)];
    if durable {
        list.push(dhnq());
    }
    c.create(Create {
        oplock: OPLOCK_LEVEL_LEASE,
        contexts: contexts(&list),
        ..mk(name)
    })
    .await
    .unwrap()
}

fn granted(resp: &smb_server::wire::messages::CreateResponse) -> u32 {
    smbc::lease::lease_reply(resp).expect("a lease").state
}

/// Two jobs with their leases in the IFS's table: another job's open
/// breaks the first job's client's write caching and waits for the
/// write-back; its write breaks the rest.
#[tokio::test(flavor = "multi_thread")]
async fn a_lease_breaks_for_another_jobs_open_and_write() {
    const RH: u32 = RWH & !smbc::lease::LEASE_WRITE_CACHING;
    let fs = Fs::new();
    let (a, b) = (job_as(&fs, "ALICE", true), job_as(&fs, "ALICE", true));
    let mut c1 = alice(a.addr, "PUB").await;
    let mut c2 = smbc::durable::user_client(
        b.addr,
        SignIn {
            share: "PUB",
            user: "alice",
            client_guid: [0xB2; 16],
            ..SignIn::default()
        },
    )
    .await;
    let r = leased_open(&mut c1, "f.txt", [1; 16], RWH, false).await;
    assert_eq!(granted(&r), RWH);
    // The other job's open waits for the cached writes to come back.
    let opening = tokio::spawn(async move {
        let id = c2
            .open("f.txt", FILE_GENERIC_READ | FILE_GENERIC_WRITE, SHARE_ALL)
            .await;
        (c2, id)
    });
    let n = c1
        .next_break(BREAK_WAIT)
        .await
        .expect("a break for the other job's open");
    assert_eq!((n.current_lease_state, n.new_lease_state), (RWH, RH));
    c1.write(r.file_id, 0, b"cached").await.unwrap();
    assert_eq!(c1.ack([1; 16], RH).await, STATUS_SUCCESS);
    let (mut c2, id) = opening.await.unwrap();
    let id = id.unwrap();
    assert_eq!(c2.read(id, 0, 6).await.unwrap(), b"cached");
    // Its write takes what is left.
    c2.write(id, 0, b"C").await.unwrap();
    let n = c1
        .next_break(BREAK_WAIT)
        .await
        .expect("a break for the other job's write");
    assert_eq!((n.current_lease_state, n.new_lease_state), (RH, 0));
    assert_eq!(c1.ack([1; 16], 0).await, STATUS_SUCCESS);
    assert_eq!(c1.read(r.file_id, 0, 6).await.unwrap(), b"Cached");
    c2.close(id).await;
    c1.close(r.file_id).await;
}

/// A plain IFS writer (no SMB at all), as FTP or a program is, breaks a
/// job's lease and waits for the write-back.
#[tokio::test(flavor = "multi_thread")]
async fn a_plain_ifs_writer_breaks_a_lease() {
    const RH: u32 = RWH & !smbc::lease::LEASE_WRITE_CACHING;
    let fs = Fs::new();
    let a = job_as(&fs, "ALICE", true);
    let mut c1 = alice(a.addr, "PUB").await;
    let r = leased_open(&mut c1, "g.txt", [2; 16], RWH, false).await;
    assert_eq!(granted(&r), RWH);
    let f = fs.clone();
    let writer = std::thread::spawn(move || {
        let me = f.client("ALICE");
        let fd = me.open("/pub/g.txt", OpenArgs::new(O_RDWR)).unwrap();
        me.pwrite(fd, b"plain", 0).unwrap();
        me.close(fd).unwrap();
        me.release();
    });
    let n = c1
        .next_break(BREAK_WAIT)
        .await
        .expect("a break for the plain writer");
    assert_eq!((n.current_lease_state, n.new_lease_state), (RWH, RH));
    c1.write(r.file_id, 5, b"!").await.unwrap();
    assert_eq!(c1.ack([2; 16], RH).await, STATUS_SUCCESS);
    let n = c1
        .next_break(BREAK_WAIT)
        .await
        .expect("a break for its write");
    assert_eq!(n.new_lease_state, 0);
    assert_eq!(c1.ack([2; 16], 0).await, STATUS_SUCCESS);
    writer.join().unwrap();
    assert_eq!(c1.read(r.file_id, 0, 6).await.unwrap(), b"plain!");
}

/// While another job has the file open a lease caches no writes.
#[tokio::test(flavor = "multi_thread")]
async fn a_lease_caches_no_writes_while_another_job_has_the_file_open() {
    let fs = Fs::new();
    let a = job_as(&fs, "ALICE", true);
    let me = fs.client("ALICE");
    let fd = me
        .open("/pub/h.txt", OpenArgs::new(O_RDWR | O_CREAT).mode(0o666))
        .unwrap();
    let mut c = alice(a.addr, "PUB").await;
    let r = leased_open(&mut c, "h.txt", [3; 16], RWH, false).await;
    assert_eq!(granted(&r), RWH & !smbc::lease::LEASE_WRITE_CACHING);
    me.close(fd).unwrap();
    me.release();
}

/// A durable open taken back on a new connection keeps its lease in the
/// table: another job's write breaks it there, and the client reads what
/// was written.
#[tokio::test(flavor = "multi_thread")]
async fn a_reconnected_durable_handle_sees_another_jobs_write() {
    let fs = Fs::new();
    let a = job_as(&fs, "ALICE", true);
    let mut c = alice(a.addr, "PUB").await;
    let r = leased_open(&mut c, "d.txt", [4; 16], RWH, true).await;
    assert_eq!(granted(&r), RWH);
    c.write(r.file_id, 0, b"before").await.unwrap();
    reset(c);
    for _ in 0..200 {
        if a.server.disconnected_opens() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut c = alice(a.addr, "PUB").await;
    let back = c
        .create(Create {
            contexts: contexts(&[lease_v1([4; 16], RWH), dhnc(r.file_id)]),
            oplock: OPLOCK_LEVEL_LEASE,
            ..mk("d.txt")
        })
        .await;
    let back = back.expect("the durable open back").file_id;
    let f = fs.clone();
    let writer = std::thread::spawn(move || {
        let me = f.client("ALICE");
        let fd = me.open("/pub/d.txt", OpenArgs::new(O_RDWR)).unwrap();
        me.pwrite(fd, b"AFTER!", 0).unwrap();
        me.close(fd).unwrap();
        me.release();
    });
    let n = c
        .next_break(BREAK_WAIT)
        .await
        .expect("a break on the new connection");
    assert_eq!(n.current_lease_state, RWH);
    assert_eq!(c.ack([4; 16], n.new_lease_state).await, STATUS_SUCCESS);
    if n.new_lease_state != 0 {
        let n = c
            .next_break(BREAK_WAIT)
            .await
            .expect("a break for the write");
        assert_eq!(c.ack([4; 16], n.new_lease_state).await, STATUS_SUCCESS);
    }
    writer.join().unwrap();
    assert_eq!(c.read(back, 0, 6).await.unwrap(), b"AFTER!");
}

/// A job that ends with leases held leaves nothing waiting.
#[tokio::test(flavor = "multi_thread")]
async fn an_ended_job_releases_its_leases() {
    let fs = Fs::new();
    let a = job_as(&fs, "ALICE", true);
    let mut c = alice(a.addr, "PUB").await;
    let r = leased_open(&mut c, "e.txt", [5; 16], RWH, false).await;
    assert_eq!(granted(&r), RWH);
    // The job's connection to the service closes: its owner is released.
    let st = fs.client("ALICE").stat("/pub/e.txt", true).unwrap();
    let owner = fs.ifs.leases().on(ifs::lock::ObjKey {
        fs: st.dev,
        ino: st.ino,
    });
    assert_eq!(owner.len(), 1);
    fs.ifs.release_owner(owner[0].0);
    let me = fs.client("ALICE");
    let fd = me.open("/pub/e.txt", OpenArgs::new(O_RDWR)).unwrap();
    me.pwrite(fd, b"x", 0).unwrap();
    me.close(fd).unwrap();
    me.release();
}

/// A `CHANGE_NOTIFY` for file and directory names on `dir`.
fn notify_body(dir: smb_server::wire::messages::FileId, tree: bool) -> Vec<u8> {
    use smb_server::wire::messages::ChangeNotifyRequest;
    let mut body = Vec::new();
    ChangeNotifyRequest {
        structure_size: 32,
        flags: if tree {
            ChangeNotifyRequest::FLAG_WATCH_TREE
        } else {
            0
        },
        output_buffer_length: 4096,
        file_id: dir,
        completion_filter: 0x3, // FILE_NAME | DIR_NAME
        reserved: 0,
    }
    .write_to(&mut body)
    .unwrap();
    body
}

/// (action, name) of each entry of a `CHANGE_NOTIFY` response body.
fn notify_entries(body: &[u8]) -> Vec<(u32, String)> {
    let buf = smb_server::wire::messages::ChangeNotifyResponse::parse(body)
        .unwrap()
        .buffer;
    let mut out = Vec::new();
    let mut at = 0;
    while at + 12 <= buf.len() {
        let field = |o: usize| u32::from_le_bytes(buf[at + o..at + o + 4].try_into().unwrap());
        let (next, action, len) = (field(0) as usize, field(4), field(8) as usize);
        let units: Vec<u16> = buf[at + 12..at + 12 + len]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        out.push((action, String::from_utf16(&units).unwrap()));
        if next == 0 {
            break;
        }
        at += next;
    }
    out
}

/// Arms a `CHANGE_NOTIFY` on `dir` and waits for its interim response.
async fn arm(c: &mut Client, dir: smb_server::wire::messages::FileId, tree: bool) -> u64 {
    let mid = c
        .send_no_wait(Command::ChangeNotify, &notify_body(dir, tree))
        .await;
    tokio::time::timeout(BREAK_WAIT, c.recv_interim(mid))
        .await
        .expect("an interim response");
    mid
}

/// What the armed `CHANGE_NOTIFY` `mid` answers.
async fn notified(c: &mut Client, mid: u64) -> Vec<(u32, String)> {
    let (status, body) = tokio::time::timeout(BREAK_WAIT, c.recv(mid, Command::ChangeNotify))
        .await
        .expect("an answer");
    assert_eq!(status, STATUS_SUCCESS);
    notify_entries(&body)
}

const ADDED: u32 = 1;

/// A watch hears what other jobs and plain IFS callers do in its
/// directory, and with a tree watch below it; the job's own changes once.
#[tokio::test(flavor = "multi_thread")]
async fn change_notify_hears_other_jobs_and_plain_ifs_changes() {
    let fs = Fs::new();
    let (a, b) = (job_as(&fs, "ALICE", true), job_as(&fs, "ALICE", true));
    let me = fs.client("ALICE");
    me.mkdir("/pub/w", 0o777).unwrap();
    me.mkdir("/pub/w/sub", 0o777).unwrap();
    let mut c = alice(a.addr, "PUB").await;
    let dir = c.open_dir("w").await;
    // A plain IFS caller (FTP, a program).
    let mid = arm(&mut c, dir, false).await;
    let fd = me
        .open(
            "/pub/w/plain.txt",
            OpenArgs::new(O_RDWR | O_CREAT).mode(0o666),
        )
        .unwrap();
    me.close(fd).unwrap();
    assert_eq!(
        notified(&mut c, mid).await,
        [(ADDED, "plain.txt".to_owned())]
    );
    // Another job's client.
    let mut other = smbc::durable::user_client(
        b.addr,
        SignIn {
            share: "PUB",
            user: "alice",
            client_guid: [0xB3; 16],
            ..SignIn::default()
        },
    )
    .await;
    let mid = arm(&mut c, dir, false).await;
    let id = other
        .create(Create {
            disposition: FILE_CREATE,
            ..mk(r"w\other.txt")
        })
        .await
        .unwrap()
        .file_id;
    other.close(id).await;
    assert_eq!(
        notified(&mut c, mid).await,
        [(ADDED, "other.txt".to_owned())]
    );
    // The job's own client: heard once, not again from the IFS.
    let mut own = alice(a.addr, "PUB").await;
    let mid = arm(&mut c, dir, false).await;
    let id = own
        .create(Create {
            disposition: FILE_CREATE,
            ..mk(r"w\own.txt")
        })
        .await
        .unwrap()
        .file_id;
    own.close(id).await;
    assert_eq!(notified(&mut c, mid).await, [(ADDED, "own.txt".to_owned())]);
    tokio::time::sleep(Duration::from_millis(400)).await;
    let mid = arm(&mut c, dir, false).await;
    me.mkdir("/pub/w/later", 0o777).unwrap();
    assert_eq!(notified(&mut c, mid).await, [(ADDED, "later".to_owned())]);
    me.release();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tree_watch_hears_other_jobs_changes_below() {
    let fs = Fs::new();
    let a = job_as(&fs, "ALICE", true);
    let me = fs.client("ALICE");
    me.mkdir("/pub/t", 0o777).unwrap();
    me.mkdir("/pub/t/sub", 0o777).unwrap();
    let mut c = alice(a.addr, "PUB").await;
    let dir = c.open_dir("t").await;
    let mid = arm(&mut c, dir, true).await;
    me.mkdir("/pub/t/sub/deep", 0o777).unwrap();
    assert_eq!(
        notified(&mut c, mid).await,
        [(ADDED, r"sub\deep".to_owned())]
    );
    me.release();
}

/// One user per share, counted across every job (the service's table).
struct OnePerShare(Arc<Mutex<Vec<String>>>);

impl netserver::TreeAdmission for OnePerShare {
    fn admit(&self, req: &netserver::TreeRequest<'_>) -> bool {
        let mut taken = self.0.lock().unwrap();
        if req.share.eq_ignore_ascii_case("IPC$")
            || !taken.iter().any(|s| s.eq_ignore_ascii_case(req.share))
        {
            taken.push(req.share.to_owned());
            true
        } else {
            false
        }
    }
}

fn admitted_job(fs: &Arc<Fs>, user: &str, taken: &Arc<Mutex<Vec<String>>>) -> Job {
    let f = fs.clone();
    let u = user.to_owned();
    let server = NetServer::build_admitted(
        &config(),
        Box::new(move || Box::new(f.client(&u)) as Box<dyn IfsClient>),
        Arc::new(Profiles),
        &(Arc::new(Spool(Arc::default())) as Arc<dyn Printer>),
        Some(Arc::new(OnePerShare(taken.clone()))),
    )
    .unwrap();
    let server = Arc::new(server);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let s = server.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let s = s.clone();
            std::thread::spawn(move || {
                let _ = s.serve_one(stream);
            });
        }
    });
    Job {
        server,
        addr,
        printed: Arc::default(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_share_s_maximum_users_hold_across_jobs_and_sessions_end() {
    let fs = Fs::new();
    let taken: Arc<Mutex<Vec<String>>> = Arc::default();
    let one = admitted_job(&fs, "ALICE", &taken);
    let two = admitted_job(&fs, "ALICE", &taken);
    let _c = alice(one.addr, "HOME").await;
    let mut s = TcpStream::connect(two.addr).await.unwrap();
    negotiate_dialects_from(&mut s, &[0x0210], [3; 16]).await;
    let l = login_with(&mut s, 1, 0, "alice", "password", LoginOptions::default()).await;
    assert_eq!(l.status, STATUS_SUCCESS);
    let r = request(
        &mut s,
        Command::TreeConnect,
        3,
        l.session_id,
        0,
        &tree_connect_body(r"\\QSTEST\HOME"),
    )
    .await;
    assert_eq!(status(&r), STATUS_REQUEST_NOT_ACCEPTED);
    // The first job's view, and QZLSENSS's end of its session.
    let view = one.server.sessions();
    assert_eq!(view.len(), 1);
    assert_eq!(view[0].user, "ALICE");
    assert_eq!(
        view[0]
            .trees
            .iter()
            .map(|t| t.share.as_str())
            .collect::<Vec<_>>(),
        ["HOME"]
    );
    assert!(one.server.end_session(view[0].session_id));
    assert!(one.server.sessions().is_empty());
}

/// A file in `ccsid` holding `data`, made by a plain IFS caller.
fn tagged(fs: &Fs, path: &str, ccsid: u16, data: &[u8]) {
    let me = fs.client("ALICE");
    let fd = me
        .open(
            path,
            OpenArgs::new(O_CREAT | O_RDWR | O_CCSID)
                .mode(0o666)
                .conv(u32::from(ccsid)),
        )
        .unwrap();
    me.pwrite(fd, data, 0).unwrap();
    me.close(fd).unwrap();
    me.release();
}

fn ccsid_of(fs: &Fs, path: &str) -> u16 {
    let me = fs.client("ALICE");
    let a = me
        .get_attrs(path, &[ifs::ops::AttrId::Ccsid], true)
        .unwrap();
    me.release();
    a.into_iter()
        .find_map(|a| {
            if let Attr::Ccsid(c) = a {
                Some(c)
            } else {
                None
            }
        })
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn text_files_convert_between_the_file_s_and_the_share_s_ccsid() {
    let fs = Fs::new();
    let ebcdic = ccsid::to_ebcdic("HELLO, WORLD").unwrap();
    tagged(&fs, "/pub/e.txt", 37, &ebcdic);
    tagged(&fs, "/pub/e.dat", 37, &ebcdic);
    let j = job(&fs, "ALICE");
    let mut c = alice(j.addr, "PUBCNV").await;
    // A listed extension: read in the share's CCSID, same size and offsets.
    let id = c
        .open("E.TXT", FILE_GENERIC_READ | FILE_GENERIC_WRITE, SHARE_ALL)
        .await
        .unwrap();
    assert_eq!(c.read(id, 0, 100).await.unwrap(), b"HELLO, WORLD");
    assert_eq!(c.read(id, 7, 5).await.unwrap(), b"WORLD");
    // Written in the share's CCSID, stored in the file's.
    assert_eq!(c.write(id, 7, b"there").await.unwrap(), 5);
    c.close(id).await;
    let me = fs.client("ALICE");
    let fd = me.open("/pub/e.txt", OpenArgs::new(O_RDWR)).unwrap();
    assert_eq!(
        me.pread(fd, 100, 0).unwrap(),
        ccsid::to_ebcdic("HELLO, there").unwrap()
    );
    me.close(fd).unwrap();
    assert_eq!(me.stat("/pub/e.txt", true).unwrap().size, 12);
    // Another extension is served as stored.
    let id = c.open("e.dat", FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
    assert_eq!(c.read(id, 0, 100).await.unwrap(), ebcdic);
    c.close(id).await;
    // A file made through the share is in the share's CCSID, unconverted.
    let id = c
        .create(Create {
            disposition: FILE_CREATE,
            ..mk("new.txt")
        })
        .await
        .unwrap()
        .file_id;
    assert_eq!(c.write(id, 0, b"abc").await.unwrap(), 3);
    c.close(id).await;
    assert_eq!(ccsid_of(&fs, "/pub/new.txt"), 819);
    let fd = me.open("/pub/new.txt", OpenArgs::new(O_RDWR)).unwrap();
    assert_eq!(me.pread(fd, 10, 0).unwrap(), b"abc");
    me.close(fd).unwrap();
    // The same files through a share without conversion.
    let mut p = alice(j.addr, "PUB").await;
    let id = p.open("e.txt", FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
    assert_eq!(
        p.read(id, 0, 5).await.unwrap(),
        ccsid::to_ebcdic("HELLO").unwrap()
    );
    p.close(id).await;
    me.release();
}

#[tokio::test(flavor = "multi_thread")]
async fn members_convert_from_their_ccsid_to_the_server_s() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let path = "qgpl.lib\\qclsrc.file\\hello.mbr";
    let mut c = alice(j.addr, "QSYSCNV").await;
    let id = c.open(path, FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
    let text = c.read(id, 0, 4096).await.unwrap();
    c.close(id).await;
    assert!(text.windows(6).any(|w| w == b"ENDPGM"), "{text:?}");
    let mut p = alice(j.addr, "QSYS").await;
    let id = p.open(path, FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
    let raw = p.read(id, 0, 4096).await.unwrap();
    p.close(id).await;
    assert_eq!(raw.len(), text.len());
    assert!(!raw.windows(6).any(|w| w == b"ENDPGM"));
}

/// A connection handed over with `serve_detached` counts at once: the job
/// is not released under it, and is once it closes.
#[tokio::test(flavor = "multi_thread")]
async fn a_detached_connection_holds_the_job_until_it_closes() {
    let fs = Fs::new();
    let j = job_as(&fs, "ALICE", false);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let accept = std::thread::spawn(move || l.accept().unwrap().0);
    let c = tokio::spawn(alice(addr, "PUB"));
    let s = tokio::task::spawn_blocking(move || accept.join().unwrap())
        .await
        .unwrap();
    j.server.serve_detached(s).unwrap();
    assert!(!j.server.release_if_idle());
    let c = c.await.unwrap();
    assert!(!j.server.release_if_idle());
    drop(c);
    let mut released = false;
    for _ in 0..100 {
        if j.server.release_if_idle() {
            released = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(released);
}

/// A server of the job `owner` with its leases and watches in the IFS.
fn server_of(fs: &Arc<Fs>, owner: u64) -> NetServer {
    let (f, g) = (fs.clone(), fs.clone());
    let server = NetServer::build(
        &config(),
        Box::new(move || Box::new(f.client_as("ALICE", owner)) as Box<dyn IfsClient>),
        Arc::new(Profiles),
        &(Arc::new(Spool(Arc::default())) as Arc<dyn Printer>),
    )
    .unwrap();
    server
        .start_leases(Box::new(move || {
            Box::new(g.client_as("ALICE", owner)) as Box<dyn IfsClient>
        }))
        .unwrap();
    server
}

/// A job serves one client after another, each with a server of its own:
/// the ended server's poll leaves first, so the next server's watch hears
/// other jobs' changes (none goes to the ended one).
#[tokio::test(flavor = "multi_thread")]
async fn a_later_server_of_the_same_job_hears_other_jobs_changes() {
    let fs = Fs::new();
    let owner = fs.owner.fetch_add(1, Ordering::SeqCst);
    let first = server_of(&fs, owner);
    tokio::time::sleep(Duration::from_millis(100)).await;
    tokio::task::spawn_blocking(move || drop(first))
        .await
        .unwrap();
    let second = Arc::new(server_of(&fs, owner));
    let me = fs.client("ALICE");
    me.mkdir("/pub/later", 0o777).unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let accept = std::thread::spawn(move || l.accept().unwrap().0);
    let c = tokio::spawn(alice(addr, "PUB"));
    let s = tokio::task::spawn_blocking(move || accept.join().unwrap())
        .await
        .unwrap();
    second.serve_detached(s).unwrap();
    let mut c = c.await.unwrap();
    let dir = c.open_dir("later").await;
    let mid = arm(&mut c, dir, false).await;
    me.mkdir("/pub/later/x", 0o777).unwrap();
    assert_eq!(notified(&mut c, mid).await, [(ADDED, "x".to_owned())]);
    me.release();
}
