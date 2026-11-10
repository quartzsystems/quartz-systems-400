//! QS/400's second round of patches (`QS400-PATCHES.md`): the guest
//! logon of a verifier, share modes and byte-range locks handed to the
//! backend, the minimum dialect, print shares, durable opens kept across
//! `serve_connection` calls, and NetBIOS keepalives.

#![cfg(feature = "localfs")]

mod common;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use common::auth::*;
use common::client::*;
use common::durable::{SignIn, contexts, dhnc, dhnq, lease_v1, negotiated, reset};
use common::lease::{OPLOCK_LEVEL_LEASE, RWH};
use common::rpc::ipc_client_as;
use common::{framed, negotiate_dialects, tree_connect_body};
use smb_server::ntstatus::*;
use smb_server::wire::header::Command;
use smb_server::wire::messages::TreeConnectResponse;
use smb_server::{
    Access, BackendCapabilities, DirEntry, FileInfo, FileTimes, Handle, Logon, LogonVerifier,
    OpenAction, OpenOptions, Share, ShareBackend, SmbPath, SmbResult, SmbServer,
};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

/// Every user is `GUESTP`'s guest: any password signs on.
struct Guests;

impl LogonVerifier for Guests {
    fn verify(&self, _: &str, _: &str, _: &[u8; 8], _: &[u8]) -> Option<Logon> {
        Some(Logon { user: "GUESTP".into(), session_base_key: [0x55; 16], guest: true })
    }
}

#[tokio::test]
async fn a_verifier_guest_gets_a_guest_session_on_its_shares() {
    let td = tempfile::tempdir().unwrap();
    std::fs::write(td.path().join("f.txt"), b"hello").unwrap();
    let (addr, _h) = common::start_server(
        SmbServer::builder()
            .logon_verifier(Guests)
            .share(Share::new("data", local(td.path())).any_user(Access::Read)),
    )
    .await;
    for dialect in [0x0210, 0x0311] {
        let mut s = TcpStream::connect(addr).await.unwrap();
        if dialect == 0x0311 {
            let body = negotiate_311_body(&[0x0311], &[preauth_context(&[1])]);
            request(&mut s, Command::Negotiate, 0, 0, 0, &body).await;
        } else {
            negotiate_dialects(&mut s, &[dialect]).await;
        }
        let login = spnego_login(&mut s, 1, 0, "anyone", "whatever").await;
        assert_eq!(login.status, STATUS_SUCCESS, "dialect {dialect:#x}");
        let flags = u16::from_le_bytes([login.frame[66], login.frame[67]]);
        assert_eq!(flags & 0x1, 0x1, "IS_GUEST");
        let tree = request(&mut s, Command::TreeConnect, 3, login.session_id, 0, &tree_connect_body(r"\\srv\data")).await;
        assert_eq!(status(&tree), STATUS_SUCCESS);
        let (h, _) = common::parse_response_header(&tree);
        assert_eq!(h.flags & 0x8, 0, "guest responses are not signed");
        let mut c = Client::from_parts(s, login.session_id, h.tree_id().unwrap());
        let id = c.open("f.txt", FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
        assert_eq!(c.read(id, 0, 5).await.unwrap(), b"hello");
    }
}

/// What a recording backend saw.
#[derive(Default)]
struct Seen {
    shares: Vec<Option<u32>>,
    locks: Vec<(u64, u64, bool)>,
    unlocks: Vec<(u64, u64)>,
    refuse_locks_at: Option<u64>,
    opened: Vec<String>,
    written: Vec<u8>,
}

/// A backend over one in-memory file per name that records share modes,
/// locks and writes.
struct Recording(Arc<Mutex<Seen>>);

struct RecHandle {
    seen: Arc<Mutex<Seen>>,
    name: String,
    index: u64,
}

#[async_trait]
impl ShareBackend for Recording {
    async fn open(&self, path: &SmbPath, opts: OpenOptions) -> SmbResult<(Box<dyn Handle>, OpenAction)> {
        let mut seen = self.0.lock().unwrap();
        let name = path.to_string();
        if !name.is_empty() && !opts.directory {
            seen.shares.push(opts.share_access);
        }
        seen.opened.push(name.clone());
        let index = seen.opened.len() as u64 + 1;
        let action = if name.is_empty() { OpenAction::Opened } else { OpenAction::Created };
        Ok((Box::new(RecHandle { seen: self.0.clone(), name, index }), action))
    }
    async fn unlink(&self, _: &SmbPath) -> SmbResult<()> {
        Ok(())
    }
    async fn rename(&self, _: &SmbPath, _: &SmbPath, _: bool) -> SmbResult<()> {
        Ok(())
    }
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }
}

#[async_trait]
impl Handle for RecHandle {
    async fn read(&self, _: u64, _: u32) -> SmbResult<bytes::Bytes> {
        Ok(bytes::Bytes::new())
    }
    async fn write(&self, _: u64, data: &[u8]) -> SmbResult<u32> {
        self.seen.lock().unwrap().written.extend_from_slice(data);
        Ok(data.len() as u32)
    }
    async fn flush(&self) -> SmbResult<()> {
        Ok(())
    }
    async fn stat(&self) -> SmbResult<FileInfo> {
        let mut info = FileInfo::new(self.name.clone());
        info.is_directory = self.name.is_empty();
        info.file_index = if self.name.is_empty() { 1 } else { self.index };
        Ok(info)
    }
    async fn set_times(&self, _: FileTimes) -> SmbResult<()> {
        Ok(())
    }
    async fn set_attributes(&self, _: u32) -> SmbResult<()> {
        Ok(())
    }
    async fn truncate(&self, _: u64) -> SmbResult<()> {
        Ok(())
    }
    async fn list_dir(&self, _: Option<&str>) -> SmbResult<Vec<DirEntry>> {
        Ok(Vec::new())
    }
    async fn close(self: Box<Self>) -> SmbResult<()> {
        Ok(())
    }
    async fn lock_range(&self, offset: u64, len: u64, exclusive: bool) -> SmbResult<()> {
        let mut seen = self.seen.lock().unwrap();
        if seen.refuse_locks_at == Some(offset) {
            return Err(smb_server::SmbError::Sharing);
        }
        seen.locks.push((offset, len, exclusive));
        Ok(())
    }
    async fn unlock_range(&self, offset: u64, len: u64) -> SmbResult<()> {
        self.seen.lock().unwrap().unlocks.push((offset, len));
        Ok(())
    }
}

async fn alice_on(builder: smb_server::SmbServerBuilder, share: &str) -> Client {
    let (addr, _h) = common::start_server(builder.user("alice", "password")).await;
    common::durable::user_client(addr, SignIn { share, ..SignIn::default() }).await
}

#[tokio::test]
async fn opens_with_data_access_hand_their_share_mode_to_the_backend() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let builder = SmbServer::builder().share(Share::new("share", Recording(seen.clone())).user("alice", Access::ReadWrite));
    let mut c = alice_on(builder, "share").await;
    let mk = |name, access, share| Create { name, access, share, disposition: FILE_OPEN_IF, ..Create::default() };
    c.create(mk("a", FILE_GENERIC_READ, SHARE_READ)).await.unwrap();
    c.create(mk("b", FILE_READ_ATTRIBUTES, 0)).await.unwrap();
    let shares = seen.lock().unwrap().shares.clone();
    assert!(shares.contains(&Some(SHARE_READ)), "{shares:?}");
    assert!(!shares.contains(&Some(0)), "an attribute-only open denies nothing: {shares:?}");
}

#[tokio::test]
async fn byte_range_locks_reach_the_backend_and_a_refusal_undoes_them() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let builder = SmbServer::builder().share(Share::new("share", Recording(seen.clone())).user("alice", Access::ReadWrite));
    let mut c = alice_on(builder, "share").await;
    let id = c.create(Create { name: "f", disposition: FILE_OPEN_IF, ..Create::default() }).await.unwrap().file_id;
    assert_eq!(c.lock(id, &[(0, 10, LOCK_EXCLUSIVE | LOCK_FAIL_IMMEDIATELY)]).await, STATUS_SUCCESS);
    assert_eq!(seen.lock().unwrap().locks, vec![(0, 10, true)]);
    assert_eq!(c.lock(id, &[(0, 10, LOCK_UNLOCK)]).await, STATUS_SUCCESS);
    assert_eq!(seen.lock().unwrap().unlocks, vec![(0, 10)]);
    seen.lock().unwrap().refuse_locks_at = Some(100);
    assert_eq!(c.lock(id, &[(100, 1, LOCK_SHARED | LOCK_FAIL_IMMEDIATELY)]).await, STATUS_LOCK_NOT_GRANTED);
    // The server's own table let it go again: the range can be taken.
    seen.lock().unwrap().refuse_locks_at = None;
    assert_eq!(c.lock(id, &[(100, 1, LOCK_EXCLUSIVE | LOCK_FAIL_IMMEDIATELY)]).await, STATUS_SUCCESS);
}

#[tokio::test]
async fn the_minimum_dialect_refuses_lower_ones() {
    let td = tempfile::tempdir().unwrap();
    let (addr, _h) = common::start_server(
        SmbServer::builder().min_dialect(0x0300).share(Share::new("data", local(td.path())).public()),
    )
    .await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    let body = common::negotiate_body(&[0x0202, 0x0210], 1, 0);
    let f = request(&mut s, Command::Negotiate, 0, 0, 0, &body).await;
    assert_eq!(status(&f), STATUS_NOT_SUPPORTED);
    let mut s = TcpStream::connect(addr).await.unwrap();
    assert_eq!(negotiate_dialects(&mut s, &[0x0210, 0x0302]).await.dialect_revision, 0x0302);
    assert!(SmbServer::builder().min_dialect(0x0201).listen(addr).build().is_err());
}

#[tokio::test]
async fn an_smb1_negotiate_for_2_002_alone_is_refused_above_it() {
    let td = tempfile::tempdir().unwrap();
    let (addr, _h) = common::start_server(
        SmbServer::builder().min_dialect(0x0210).share(Share::new("data", local(td.path())).public()),
    )
    .await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    let mut smb1 = vec![0xFF, b'S', b'M', b'B', 0x72];
    smb1.resize(32, 0);
    smb1.push(0);
    let dialects = b"\x02NT LM 0.12\0\x02SMB 2.002\0";
    smb1.extend_from_slice(&(dialects.len() as u16).to_le_bytes());
    smb1.extend_from_slice(dialects);
    s.write_all(&framed(&smb1)).await.unwrap();
    assert!(read_or_closed(&mut s).await.is_none(), "closed without a reply");
}

#[tokio::test]
async fn a_print_share_makes_a_print_file_of_every_create() {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let (addr, _h) = common::start_server(
        SmbServer::builder()
            .user("alice", "password")
            .share(Share::new("PRT01", Recording(seen.clone())).print().any_user(Access::ReadWrite).remark("Printer")),
    )
    .await;
    let mut s = negotiated(addr, 0x0210, [0xA1; 16]).await;
    let login = login_with(&mut s, 1, 0, "alice", "password", LoginOptions::default()).await;
    let tree = request(&mut s, Command::TreeConnect, 3, login.session_id, 0, &tree_connect_body(r"\\srv\PRT01")).await;
    assert_eq!(status(&tree), STATUS_SUCCESS);
    let (h, body) = common::parse_response_header(&tree);
    assert_eq!(TreeConnectResponse::parse(body).unwrap().share_type, TreeConnectResponse::SHARE_TYPE_PRINT);
    let mut c = Client::from_parts(s, login.session_id, h.tree_id().unwrap());
    for _ in 0..2 {
        let id = c
            .create(Create { name: "doc", access: FILE_GENERIC_WRITE, disposition: FILE_OVERWRITE_IF, ..Create::default() })
            .await
            .unwrap()
            .file_id;
        c.write(id, 0, b"\x1bE").await.unwrap();
        c.close(id).await;
    }
    let seen = seen.lock().unwrap();
    let print: Vec<&String> = seen.opened.iter().filter(|n| n.starts_with("PRINT")).collect();
    assert!(print.contains(&&"PRINT00000001".to_string()) && print.contains(&&"PRINT00000002".to_string()), "{:?}", seen.opened);
    assert_eq!(seen.written, b"\x1bE\x1bE");
    drop(seen);
    let dir = c
        .create(Create { name: "x", access: FILE_GENERIC_READ, options: FILE_DIRECTORY_FILE, disposition: FILE_OPEN_IF, ..Create::default() })
        .await;
    assert_eq!(dir.err(), Some(STATUS_INVALID_PARAMETER));

    let mut ipc = ipc_client_as(addr, 0x0210, Some(("alice", "password"))).await;
    let pipe = ipc.srvsvc().await;
    let shares = ipc.list_shares(pipe, 1).await;
    let prt = shares.entries.iter().find(|e| e.name == "PRT01").unwrap();
    assert_eq!(prt.kind, Some(1), "STYPE_PRINTQ");
}

/// A server whose connections are handed over one at a time.
async fn handed_over() -> (Arc<SmbServer>, SocketAddr, tempfile::TempDir) {
    let td = tempfile::tempdir().unwrap();
    let server = SmbServer::builder()
        .user("alice", "password")
        .share(Share::new("share", local(td.path())).user("alice", Access::ReadWrite))
        .durable_handle_timeout(Duration::from_millis(600))
        .listen("127.0.0.1:0".parse().unwrap())
        .build()
        .unwrap();
    let server = Arc::new(server);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let srv = server.clone();
    tokio::spawn(async move {
        loop {
            let (stream, peer) = listener.accept().await.unwrap();
            let srv = srv.clone();
            tokio::spawn(async move { srv.serve_connection(stream, peer).await });
        }
    });
    (server, addr, td)
}

#[tokio::test]
async fn durable_opens_outlive_one_handed_over_connection() {
    let (server, addr, _td) = handed_over().await;
    let guid = [0xA1; 16];
    let mut c = common::durable::user_client(addr, SignIn::default()).await;
    assert_eq!(server.client_guids().await, vec![guid]);
    let resp = c
        .create(Create {
            name: "f",
            oplock: OPLOCK_LEVEL_LEASE,
            disposition: FILE_OPEN_IF,
            contexts: contexts(&[lease_v1([0xA5; 16], RWH), dhnq()]),
            ..Create::default()
        })
        .await
        .unwrap();
    reset(c);
    assert!(eventually(|| server.disconnected_opens() == 1).await);
    assert_eq!(server.client_guids().await, vec![guid]);
    // The same client comes back on another connection and takes it back.
    let mut c = common::durable::user_client(addr, SignIn::default()).await;
    let back = c
        .create(Create {
            name: "f",
            oplock: OPLOCK_LEVEL_LEASE,
            contexts: contexts(&[lease_v1([0xA5; 16], RWH), dhnc(resp.file_id)]),
            ..Create::default()
        })
        .await;
    assert!(back.is_ok(), "{back:?}");
    assert_eq!(server.disconnected_opens(), 0);
    reset(c);
    assert!(eventually(|| server.disconnected_opens() == 1).await);
    // Then it expires; finish returns once it is closed.
    tokio::time::timeout(Duration::from_secs(10), server.finish()).await.unwrap();
    assert_eq!(server.disconnected_opens(), 0);
}

#[tokio::test]
async fn a_netbios_keepalive_between_messages_is_skipped() {
    let td = tempfile::tempdir().unwrap();
    let (addr, _h) = common::start_server(SmbServer::builder().share(Share::new("data", local(td.path())).public())).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(&[0x85, 0, 0, 0]).await.unwrap();
    negotiate_dialects(&mut s, &[0x0210]).await;
    s.write_all(&[0x85, 0, 0, 0]).await.unwrap();
    assert_ne!(common::anonymous_session_setup(&mut s).await, 0);
}

#[tokio::test]
async fn the_server_comment_is_reported() {
    use common::rpc::{OPNUM_SERVER_GET_INFO, decode_server_info, server_info_request};
    let td = tempfile::tempdir().unwrap();
    let (addr, _h) = common::start_server(
        SmbServer::builder()
            .user("alice", "password")
            .domain("QSDOMAIN")
            .comment("QS/400 test system")
            .share(Share::new("data", local(td.path())).any_user(Access::Read)),
    )
    .await;
    let mut ipc = ipc_client_as(addr, 0x0210, Some(("alice", "password"))).await;
    let pipe = ipc.srvsvc().await;
    let stub = ipc.call(pipe, 9, OPNUM_SERVER_GET_INFO, &server_info_request(101)).await.unwrap();
    let (_, info, _) = decode_server_info(&stub);
    assert_eq!(info.unwrap().details.unwrap().3, "QS/400 test system");
}
