//! QS/400's leases shared with other processes (`QS400-PATCHES.md`): the
//! observer hears every change to a file's leases, a backend caps a lease
//! before it is granted, and a break ordered from outside reaches the
//! client as a change through another open would.

#![cfg(feature = "localfs")]

mod common;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use common::client::*;
use common::lease::*;
use smb_server::{
    BackendCapabilities, DirEntry, FileInfo, FileTimes, Handle, LeaseKey, LeaseObserver,
    OpenAction, OpenOptions, Share, ShareBackend, SmbPath, SmbResult, SmbServer, SmbServerBuilder,
};
use tokio::net::TcpListener;

const KEY: [u8; 16] = [0xA1; 16];
const WAIT: Duration = Duration::from_secs(5);

/// Every report: share, file, leases.
type Reports = Arc<Mutex<Vec<(String, u64, Vec<(LeaseKey, u32)>)>>>;

struct Recorder(Reports);

impl LeaseObserver for Recorder {
    fn leases(&self, share: &str, file_index: u64, leases: &[(LeaseKey, u32)]) {
        self.0
            .lock()
            .unwrap()
            .push((share.into(), file_index, leases.to_vec()));
    }
}

/// The server, its address, served one connection at a time per task.
async fn serve(builder: SmbServerBuilder) -> (Arc<SmbServer>, SocketAddr) {
    let smb = Arc::new(
        builder
            .listen("127.0.0.1:0".parse().unwrap())
            .build()
            .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let s = smb.clone();
    tokio::spawn(async move {
        while let Ok((stream, peer)) = listener.accept().await {
            let s = s.clone();
            tokio::spawn(async move {
                let _ = s.serve_connection(stream, peer).await;
            });
        }
    });
    (smb, addr)
}

fn last(r: &Reports) -> (u64, Vec<(LeaseKey, u32)>) {
    let r = r.lock().unwrap();
    let (_, index, leases) = r.last().expect("a report").clone();
    (index, leases)
}

#[tokio::test]
async fn the_observer_hears_grants_and_breaks_from_outside_reach_the_client() {
    let td = tempfile::tempdir().unwrap();
    std::fs::write(td.path().join("f.txt"), b"data").unwrap();
    let reports: Reports = Arc::default();
    let (smb, addr) = serve(
        SmbServer::builder()
            .lease_observer(Recorder(reports.clone()))
            .share(Share::new("share", local(td.path())).public()),
    )
    .await;
    let mut c = Client::connect(addr, None).await;
    let resp = c.create(leased("f.txt", rqls_v1(KEY, RWH))).await.unwrap();
    assert_eq!(lease_reply(&resp).unwrap().state, RWH);
    let (index, leases) = last(&reports);
    assert_eq!(reports.lock().unwrap().last().unwrap().0, "share");
    assert_eq!(leases.len(), 1);
    assert_eq!((leases[0].0.key, leases[0].1), (KEY, RWH));
    // Another process wants the file: write caching goes, after a write-back.
    assert!(
        smb.break_leases(index, LEASE_READ_CACHING | LEASE_HANDLE_CACHING)
            .await
    );
    let n = c.next_break(WAIT).await.expect("a break");
    assert_eq!(
        (n.current_lease_state, n.new_lease_state),
        (RWH, LEASE_READ_CACHING | LEASE_HANDLE_CACHING)
    );
    // Until the client acknowledges, the lease keeps what it caches.
    assert_eq!(last(&reports).1[0].1, RWH);
    c.write(resp.file_id, 0, b"D").await.unwrap();
    assert_eq!(
        c.ack(KEY, LEASE_READ_CACHING | LEASE_HANDLE_CACHING).await,
        0
    );
    assert_eq!(
        last(&reports).1[0].1,
        LEASE_READ_CACHING | LEASE_HANDLE_CACHING
    );
    // A break that changes nothing is reported all the same.
    let before = reports.lock().unwrap().len();
    assert!(smb.break_leases(index, RWH).await);
    assert_eq!(reports.lock().unwrap().len(), before + 1);
    c.close(resp.file_id).await;
    assert!(last(&reports).1.is_empty(), "the lease ended with its open");
    assert!(!smb.break_leases(index, 0).await, "nothing open");
}

/// One in-memory file per name; leases capped at `cap`.
struct Capped {
    cap: u32,
    asked: Arc<Mutex<Vec<(LeaseKey, u32)>>>,
}

struct CappedHandle {
    cap: u32,
    asked: Arc<Mutex<Vec<(LeaseKey, u32)>>>,
    name: String,
}

#[async_trait]
impl ShareBackend for Capped {
    async fn open(
        &self,
        path: &SmbPath,
        _: OpenOptions,
    ) -> SmbResult<(Box<dyn Handle>, OpenAction)> {
        let name = path.to_string();
        Ok((
            Box::new(CappedHandle {
                cap: self.cap,
                asked: self.asked.clone(),
                name,
            }),
            OpenAction::Opened,
        ))
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
impl Handle for CappedHandle {
    async fn read(&self, _: u64, _: u32) -> SmbResult<bytes::Bytes> {
        Ok(bytes::Bytes::new())
    }
    async fn write(&self, _: u64, data: &[u8]) -> SmbResult<u32> {
        Ok(data.len() as u32)
    }
    async fn flush(&self) -> SmbResult<()> {
        Ok(())
    }
    async fn stat(&self) -> SmbResult<FileInfo> {
        let mut info = FileInfo::new(self.name.clone());
        info.is_directory = self.name.is_empty();
        info.file_index = if self.name.is_empty() { 1 } else { 2 };
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
    async fn lease_limit(&self, lease: LeaseKey, want: u32) -> u32 {
        self.asked.lock().unwrap().push((lease, want));
        self.cap
    }
}

#[tokio::test]
async fn a_backend_caps_what_a_lease_is_granted() {
    let asked = Arc::default();
    let (_smb, addr) = serve(
        SmbServer::builder().share(
            Share::new(
                "share",
                Capped {
                    cap: LEASE_READ_CACHING | LEASE_HANDLE_CACHING,
                    asked: Arc::clone(&asked),
                },
            )
            .public(),
        ),
    )
    .await;
    let mut c = Client::connect(addr, None).await;
    let resp = c.create(leased("f.txt", rqls_v1(KEY, RWH))).await.unwrap();
    assert_eq!(
        lease_reply(&resp).unwrap().state,
        LEASE_READ_CACHING | LEASE_HANDLE_CACHING
    );
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(
        (asked[0].0.key, asked[0].0.client, asked[0].1),
        (KEY, common::CLIENT_GUID, RWH)
    );
}
