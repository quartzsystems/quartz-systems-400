// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NetServer` end to end: SMB clients (the library's own test helpers)
//! against `NetServer` jobs over one IFS in memory, through handed-over
//! loopback sockets as `QZLSSERVER` hands them.

#![allow(clippy::many_single_char_names)]

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
use ifs::ops::{Attr, Config as IfsConfig, Ctx, Ifs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData};
use md4::{Digest, Md4};
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
    let mut p = Principal { user: user.into(), qsecurity: 40, objaud: "*NONE".into(), ..Principal::default() };
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
            MemberData { records: vec![rec("PGM"), rec("ENDPGM")], record_len: 80, source: true, ccsid: 37 },
        );
        let env = MemEnv::default().user("QSYS", 0).user("QSECOFR", 1).user("QDFTOWN", 3).user("ALICE", 110).user("BOB", 120);
        let ifs = Ifs::new(MemBackend::new(), Box::new(q), IfsConfig::default());
        ifs.ensure_layout(&env).unwrap();
        let audit = MemAudit::default();
        let sec = principal("QSECOFR");
        let cx = Ctx { owner: Owner(u64::MAX), principal: &sec, env: &env, audit: &audit, job_ccsid: 37 };
        ifs.mkdir(&cx, "/home/ALICE", 0o755).unwrap();
        ifs.chgown(&cx, "/home/ALICE", "ALICE", true).unwrap();
        ifs.mkdir(&cx, "/pub", 0o777).unwrap();
        ifs.chmod(&cx, "/pub", 0o777).unwrap();
        ifs.release_owner(Owner(u64::MAX));
        Arc::new(Self { ifs: Arc::new(ifs), env: Arc::new(env), audit: Arc::new(audit), owner: AtomicU64::new(1) })
    }

    fn client(&self, user: &str) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: self.ifs.clone(),
            env: self.env.clone(),
            audit: self.audit.clone(),
            principal: principal(user),
            owner: Owner(self.owner.fetch_add(1, Ordering::SeqCst)),
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
            return Err(Refusal::WrongPassword { disabled_now: false });
        }
        Ok(SignedOn { profile, guest: false, session_base_key: Some(hmac(&key, &[&computed])) })
    }
}

/// What was printed: user, queue, document, data.
type Printed = Arc<Mutex<Vec<(String, String, String, Vec<u8>)>>>;

struct Spool(Printed);
struct SpoolJob(Printed, (String, String, String), Vec<u8>);

impl Printer for Spool {
    fn open(&self, user: &str, q: &OutQueue, doc: &str) -> Result<Box<dyn PrintJob>, PrintError> {
        Ok(Box::new(SpoolJob(self.0.clone(), (user.into(), format!("{}/{}", q.library, q.name), doc.into()), Vec::new())))
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
    NetServerConfig {
        server_name: "QSTEST".into(),
        file_shares: vec![FileShare::new("HOME", "/home/ALICE"), FileShare::new("PUB", "/pub"), ro, FileShare::new("QSYS", "/QSYS.LIB")],
        print_shares: vec![PrintShareCfg {
            name: "PRT01".into(),
            text: "Printer".into(),
            queue: OutQueue { library: "QGPL".into(), name: "PRT01".into() },
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
    let printed: Printed = Arc::default();
    let f = fs.clone();
    let u = user.to_owned();
    let server = NetServer::build(
        &config(),
        Box::new(move || Box::new(f.client(&u)) as Box<dyn IfsClient>),
        Arc::new(Profiles),
        &(Arc::new(Spool(printed.clone())) as Arc<dyn Printer>),
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
    Job { server, addr, printed }
}

async fn alice(addr: SocketAddr, share: &str) -> Client {
    smbc::durable::user_client(addr, SignIn { share, user: "alice", ..SignIn::default() }).await
}

fn mk(name: &str) -> Create<'_> {
    Create { name, disposition: FILE_OPEN_IF, ..Create::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn files_round_trip_through_the_ifs() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut c = alice(j.addr, "HOME").await;
    let id = c.create(Create { disposition: FILE_CREATE, ..mk("Notes.txt") }).await.unwrap().file_id;
    assert_eq!(c.write(id, 0, b"hello, world").await.unwrap(), 12);
    c.close(id).await;
    // The IFS holds it, owned by the profile.
    let me = fs.client("ALICE");
    let st = me.stat("/home/ALICE/Notes.txt", true).unwrap();
    assert_eq!(st.size, 12);
    assert_eq!(st.owner, "ALICE");
    // Any case opens it; the listing keeps the case it was made with.
    let id = c.open("NOTES.TXT", FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
    assert_eq!(c.read(id, 7, 5).await.unwrap(), b"world");
    c.close(id).await;
    let dir = c.open_dir("").await;
    assert_eq!(c.list(dir, 0, "*").await.unwrap(), [".", "..", "Notes.txt"]);
    assert_eq!(c.list(dir, 0x01, "*.TXT").await.unwrap(), ["Notes.txt"]);
    c.close(dir).await;
    // A directory, a rename into it, a delete.
    let d = c.create(Create { access: FILE_GENERIC_READ, options: FILE_DIRECTORY_FILE, disposition: FILE_CREATE, ..mk("docs") }).await.unwrap().file_id;
    c.close(d).await;
    let id = c.open("Notes.txt", DELETE | FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
    let mut rename = vec![0u8; 20];
    let target = smbc::utf16le("docs\\N2.txt");
    rename[16..20].copy_from_slice(&u32::try_from(target.len()).unwrap().to_le_bytes());
    rename.extend_from_slice(&target);
    assert_eq!(c.set_info(id, FILE_RENAME_INFORMATION, rename).await, STATUS_SUCCESS);
    c.close(id).await;
    assert!(me.stat("/home/ALICE/docs/N2.txt", true).is_ok());
    let id = c.open("docs\\n2.TXT", DELETE, SHARE_ALL).await.unwrap();
    assert_eq!(c.set_delete(id, true).await, STATUS_SUCCESS);
    c.close(id).await;
    assert!(me.stat("/home/ALICE/docs/N2.txt", true).is_err());
    assert_eq!(c.open("nothere\\x", FILE_GENERIC_READ, SHARE_ALL).await.err(), Some(STATUS_OBJECT_PATH_NOT_FOUND));
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
    me.set_attr("/home/ALICE/a.txt", &Attr::PcHidden(true), true).unwrap();
    let id = c.open("a.txt", FILE_GENERIC_READ | FILE_WRITE_ATTRIBUTES, SHARE_ALL).await.unwrap();
    let basic = c.query_file_info(id, FILE_BASIC_INFORMATION).await.unwrap();
    let attrs = u32_at(&basic, 32);
    assert_eq!(attrs & 0x2, 0x2, "hidden");
    // Read-only and archive set through SMB land in the IFS.
    let mut set = basic.clone();
    set[32..36].copy_from_slice(&(0x1u32 | 0x20).to_le_bytes());
    assert_eq!(c.set_info(id, FILE_BASIC_INFORMATION, set).await, STATUS_SUCCESS);
    c.close(id).await;
    let got = me.get_attrs("/home/ALICE/a.txt", &[ifs::ops::AttrId::PcReadOnly, ifs::ops::AttrId::PcHidden, ifs::ops::AttrId::PcArchive], true).unwrap();
    assert!(got.contains(&Attr::PcReadOnly(true)) && got.contains(&Attr::PcArchive(true)) && got.contains(&Attr::PcHidden(false)), "{got:?}");
    // The file ID is the IFS's, the same in listings and opens.
    let id = c.open("a.txt", FILE_READ_ATTRIBUTES, SHARE_ALL).await.unwrap();
    let internal = u64_at(&c.query_file_info(id, FILE_INTERNAL_INFORMATION).await.unwrap(), 0);
    c.close(id).await;
    let dir = c.open_dir("").await;
    let (_, buf) = c.query_directory(dir, FILE_ID_BOTH_DIRECTORY_INFORMATION, 0, "a.txt", 65536).await;
    assert_eq!(smbc::id_both_entries(&buf)[0].1, internal);
    me.release();
}

#[tokio::test(flavor = "multi_thread")]
async fn share_modes_and_locks_reach_other_jobs() {
    let fs = Fs::new();
    let (a, b) = (job(&fs, "ALICE"), job(&fs, "ALICE"));
    let mut c1 = alice(a.addr, "PUB").await;
    let mut c2 = alice(b.addr, "PUB").await;
    let id = c1.create(Create { share: SHARE_READ, ..mk("shared.dat") }).await.unwrap().file_id;
    c1.write(id, 0, &[0; 100]).await.unwrap();
    // The other job may read, not write.
    assert_eq!(c2.open("shared.dat", FILE_GENERIC_WRITE, SHARE_ALL).await.err(), Some(STATUS_SHARING_VIOLATION));
    // A reader that denies writing conflicts with the writer.
    let r = c2.open("shared.dat", FILE_GENERIC_READ, SHARE_READ).await;
    assert_eq!(r.err(), Some(STATUS_SHARING_VIOLATION));
    let r = c2.open("shared.dat", FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
    // A byte-range lock in one job refuses the other's.
    assert_eq!(c1.lock(id, &[(0, 10, LOCK_EXCLUSIVE | LOCK_FAIL_IMMEDIATELY)]).await, STATUS_SUCCESS);
    assert_eq!(c2.lock(r, &[(5, 10, LOCK_SHARED | LOCK_FAIL_IMMEDIATELY)]).await, STATUS_LOCK_NOT_GRANTED);
    assert_eq!(c1.lock(id, &[(0, 10, LOCK_UNLOCK)]).await, STATUS_SUCCESS);
    assert_eq!(c2.lock(r, &[(5, 10, LOCK_SHARED | LOCK_FAIL_IMMEDIATELY)]).await, STATUS_SUCCESS);
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
    let mut c = smbc::durable::user_client(b.addr, SignIn { share: "HOME", user: "bob", client_guid: [0xB2; 16], ..SignIn::default() }).await;
    assert_eq!(c.create(mk("bobs.txt")).await.err(), Some(STATUS_ACCESS_DENIED));
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
    assert_eq!(l.status, STATUS_LOGON_FAILURE, "a second profile in the job");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_print_share_spools_as_the_profile() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut s = smbc::durable::negotiated(j.addr, 0x0210, [0xA1; 16]).await;
    let l = login_with(&mut s, 1, 0, "alice", "password", LoginOptions::default()).await;
    let t = request(&mut s, Command::TreeConnect, 3, l.session_id, 0, &tree_connect_body(r"\\QSTEST\PRT01")).await;
    assert_eq!(status(&t), STATUS_SUCCESS);
    let (h, body) = smbc::parse_response_header(&t);
    assert_eq!(smb_server::wire::messages::TreeConnectResponse::parse(body).unwrap().share_type, 3);
    let mut c = Client::from_parts(s, l.session_id, h.tree_id().unwrap());
    let id = c.create(Create { access: FILE_GENERIC_WRITE, disposition: FILE_OVERWRITE_IF, ..mk("Report") }).await.unwrap().file_id;
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
    let id = c.open("qgpl.lib\\qclsrc.file\\hello.mbr", FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
    let data = c.read(id, 0, 4096).await.unwrap();
    assert!(!data.is_empty());
    c.close(id).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_routed_reconnect_takes_its_durable_open_back() {
    let fs = Fs::new();
    let j = job(&fs, "ALICE");
    let mut c = alice(j.addr, "PUB").await;
    let leased = |name| Create { oplock: OPLOCK_LEVEL_LEASE, contexts: contexts(&[lease_v1([5; 16], RWH), dhnq()]), ..mk(name) };
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
        .create(Create { contexts: contexts(&[lease_v1([5; 16], RWH), dhnc(resp.file_id)]), oplock: OPLOCK_LEVEL_LEASE, ..mk("durable.txt") })
        .await;
    assert!(back.is_ok(), "{back:?}");
    drop(c);
    let server = j.server.clone();
    tokio::task::spawn_blocking(move || server.idle_until_released()).await.unwrap();
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
    let req = nbss::session_request(&nbss::NbName::new("QSTEST", nbss::SUFFIX_SERVER), &nbss::NbName::new("PC", 0));
    s.write_all(&req).await.unwrap();
    let mut resp = [0u8; 4];
    s.read_exact(&mut resp).await.unwrap();
    assert_eq!(resp, nbss::positive_response());
    negotiate_dialects_from(&mut s, &[0x0210], [9; 16]).await;
    s.write_all(&[nbss::KEEP_ALIVE, 0, 0, 0]).await.unwrap();
    let l = login_with(&mut s, 1, 0, "alice", "password", LoginOptions::default()).await;
    assert_eq!(l.status, STATUS_SUCCESS);
    let t = request(&mut s, Command::TreeConnect, 3, l.session_id, 0, &tree_connect_body(r"\\QSTEST\PUB")).await;
    assert_eq!(status(&t), STATUS_SUCCESS);
}
