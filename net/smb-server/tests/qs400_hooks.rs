//! QS/400's hooks (`QS400-PATCHES.md`): a logon verifier, the any-user
//! share grant, and serving one connection accepted elsewhere.

#![cfg(feature = "localfs")]

mod common;

use std::net::SocketAddr;

use common::auth::*;
use common::{negotiate, start_server, tree_connect_body};
use hmac::{Hmac, Mac};
use md4::{Digest, Md4};
use smb_server::ntstatus::{STATUS_ACCESS_DENIED, STATUS_LOGON_FAILURE, STATUS_SUCCESS};
use smb_server::wire::header::Command;
use smb_server::{Access, LocalFsBackend, Logon, LogonVerifier, Share, SmbServer};
use tokio::net::{TcpListener, TcpStream};

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

/// Knows one user, `QSUSER` with password `secret`, under any spelling, and
/// names the session `QSUSER` as a profile would be.
struct OneProfile;

impl LogonVerifier for OneProfile {
    fn verify(&self, user: &str, domain: &str, challenge: &[u8; 8], resp: &[u8]) -> Option<Logon> {
        if !user.eq_ignore_ascii_case("qsuser") || resp.len() < 16 {
            return None;
        }
        let nt: [u8; 16] = Md4::digest(utf16("secret")).into();
        let key = hmac(&nt, &[&utf16(&format!("{}{domain}", user.to_uppercase()))]);
        let (proof, blob) = resp.split_at(16);
        let computed = hmac(&key, &[challenge, blob]);
        (proof == computed).then(|| Logon { user: "QSUSER".into(), session_base_key: hmac(&key, &[&computed]), guest: false })
    }
}

fn builder(td: &tempfile::TempDir, access: Access) -> smb_server::SmbServerBuilder {
    SmbServer::builder()
        .logon_verifier(OneProfile)
        .share(Share::new("data", LocalFsBackend::new(td.path()).unwrap()).any_user(access))
}

async fn tree(s: &mut TcpStream, mid: u64, sid: u64) -> u32 {
    status(&request(s, Command::TreeConnect, mid, sid, 0, &tree_connect_body(r"\\srv\data")).await)
}

#[tokio::test]
async fn verifier_accepts_and_names_the_session() {
    let td = tempfile::tempdir().unwrap();
    let (addr, _h) = start_server(builder(&td, Access::ReadWrite)).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    negotiate(&mut s).await;
    let login = spnego_login(&mut s, 1, 0, "qsuser", "secret").await;
    assert_eq!(login.status, STATUS_SUCCESS);
    assert_eq!(tree(&mut s, 3, login.session_id).await, STATUS_SUCCESS);
}

#[tokio::test]
async fn verifier_refuses_a_wrong_password() {
    let td = tempfile::tempdir().unwrap();
    let (addr, _h) = start_server(builder(&td, Access::ReadWrite)).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    negotiate(&mut s).await;
    assert_eq!(spnego_login(&mut s, 1, 0, "qsuser", "wrong").await.status, STATUS_LOGON_FAILURE);
    let mut s = TcpStream::connect(addr).await.unwrap();
    negotiate(&mut s).await;
    assert_eq!(spnego_login(&mut s, 1, 0, "nobody", "secret").await.status, STATUS_LOGON_FAILURE);
}

#[tokio::test]
async fn any_user_grant_without_a_verifier() {
    let td = tempfile::tempdir().unwrap();
    let (addr, _h) = start_server(
        SmbServer::builder()
            .user("alice", "pw")
            .share(Share::new("data", LocalFsBackend::new(td.path()).unwrap()).any_user(Access::Read)),
    )
    .await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    negotiate(&mut s).await;
    let login = spnego_login(&mut s, 1, 0, "alice", "pw").await;
    assert_eq!(login.status, STATUS_SUCCESS);
    assert_eq!(tree(&mut s, 3, login.session_id).await, STATUS_SUCCESS);
}

#[tokio::test]
async fn closed_share_still_refuses_others() {
    let td = tempfile::tempdir().unwrap();
    let (addr, _h) = start_server(
        SmbServer::builder()
            .user("alice", "pw")
            .user("bob", "pw")
            .share(Share::new("data", LocalFsBackend::new(td.path()).unwrap()).user("alice", Access::Read)),
    )
    .await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    negotiate(&mut s).await;
    let login = spnego_login(&mut s, 1, 0, "bob", "pw").await;
    assert_eq!(login.status, STATUS_SUCCESS);
    assert_eq!(tree(&mut s, 3, login.session_id).await, STATUS_ACCESS_DENIED);
}

#[tokio::test]
async fn serve_connection_serves_one_handed_over_socket() {
    let td = tempfile::tempdir().unwrap();
    let server = builder(&td, Access::ReadWrite).listen("127.0.0.1:0".parse().unwrap()).build().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.unwrap();
        server.serve_connection(stream, peer).await
    });
    let mut s = TcpStream::connect(addr).await.unwrap();
    negotiate(&mut s).await;
    let login = spnego_login(&mut s, 1, 0, "QSUSER", "secret").await;
    assert_eq!(login.status, STATUS_SUCCESS);
    assert_eq!(tree(&mut s, 3, login.session_id).await, STATUS_SUCCESS);
    drop(s);
    task.await.unwrap().unwrap();
}

/// QS/400: admits `data` once; records what it was asked.
struct OnceOnly(std::sync::Arc<std::sync::Mutex<Vec<(String, String, u8)>>>);

impl smb_server::TreeAdmission for OnceOnly {
    fn admit(&self, req: &smb_server::TreeRequest<'_>) -> bool {
        let mut seen = self.0.lock().unwrap();
        let first = !seen.iter().any(|(_, s, _)| s == req.share);
        seen.push((req.user.to_owned(), req.share.to_owned(), req.kind));
        first
    }
}

#[tokio::test]
async fn tree_admission_and_the_sessions_view() {
    use smb_server::ntstatus::STATUS_REQUEST_NOT_ACCEPTED;
    let td = tempfile::tempdir().unwrap();
    let seen: std::sync::Arc<std::sync::Mutex<Vec<(String, String, u8)>>> = std::sync::Arc::default();
    let server = std::sync::Arc::new(
        builder(&td, Access::ReadWrite)
            .tree_admission(OnceOnly(seen.clone()))
            .listen("127.0.0.1:0".parse().unwrap())
            .build()
            .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let srv = server.clone();
    let task = tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.unwrap();
        srv.serve_connection(stream, peer).await
    });
    let mut s = TcpStream::connect(addr).await.unwrap();
    negotiate(&mut s).await;
    let login = spnego_login(&mut s, 1, 0, "qsuser", "secret").await;
    assert_eq!(tree(&mut s, 3, login.session_id).await, STATUS_SUCCESS);
    assert_eq!(tree(&mut s, 4, login.session_id).await, STATUS_REQUEST_NOT_ACCEPTED);
    assert_eq!(seen.lock().unwrap()[0], ("QSUSER".to_owned(), "data".to_owned(), 1));
    let view = server.sessions().await;
    assert_eq!(view.len(), 1);
    assert_eq!(view[0].user, "QSUSER");
    assert_eq!(view[0].trees.len(), 1);
    assert_eq!(view[0].trees[0].share, "data");
    assert!(server.end_session(login.session_id).await);
    assert!(server.sessions().await.is_empty());
    assert!(!server.end_session(login.session_id).await);
    drop(s);
    task.await.unwrap().unwrap();
}
