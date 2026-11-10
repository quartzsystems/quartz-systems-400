//! QS/400's tree changes from a backend (`QS400-PATCHES.md`): a backend
//! watch may report a change below the watched directory by its path
//! there, and only tree watches hear it.

#![cfg(feature = "localfs")]

mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use common::client::*;
use common::notify::*;
use smb_server::{
    BackendCapabilities, DirChange, DirChangeKind, DirEntry, DirWatch, FileInfo, FileTimes, Handle,
    OpenAction, OpenOptions, Share, ShareBackend, SmbPath, SmbResult, SmbServer,
};
use tokio::sync::mpsc;

type Feed = Arc<Mutex<Option<mpsc::UnboundedReceiver<Vec<DirChange>>>>>;

/// A share of one empty directory whose watch reports what the test sends.
struct Fed(Feed);

struct FedHandle(Feed);

struct FedWatch(mpsc::UnboundedReceiver<Vec<DirChange>>);

#[async_trait]
impl DirWatch for FedWatch {
    async fn next(&mut self) -> Option<Vec<DirChange>> {
        self.0.recv().await
    }
}

#[async_trait]
impl ShareBackend for Fed {
    async fn open(&self, _: &SmbPath, _: OpenOptions) -> SmbResult<(Box<dyn Handle>, OpenAction)> {
        Ok((Box::new(FedHandle(self.0.clone())), OpenAction::Opened))
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
impl Handle for FedHandle {
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
        let mut info = FileInfo::new(String::new());
        info.is_directory = true;
        info.file_index = 1;
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
    async fn watch(&self) -> SmbResult<Option<Box<dyn DirWatch>>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .take()
            .map(|rx| Box::new(FedWatch(rx)) as Box<dyn DirWatch>))
    }
}

#[tokio::test]
async fn changes_below_a_watched_directory_reach_tree_watches_only() {
    let (tx, rx) = mpsc::unbounded_channel();
    let feed: Feed = Arc::new(Mutex::new(Some(rx)));
    let server =
        Server::start(SmbServer::builder().share(Share::new("share", Fed(feed)).public())).await;
    let mut tree = server.client().await;
    let mut flat = server.client().await;
    let t = tree.open_dir("").await;
    let f = flat.open_dir("").await;
    let mt = arm(&mut tree, t, 4096, NAMES, true).await;
    let mf = arm(&mut flat, f, 4096, NAMES, false).await;
    tx.send(vec![
        DirChange::new("sub\\new.txt", DirChangeKind::Added, false),
        DirChange::new("sub\\bad\"", DirChangeKind::Added, false),
        DirChange::new("top.txt", DirChangeKind::Added, false),
    ])
    .unwrap();
    let (_, got) = wait_notify(&mut tree, mt, 4096).await;
    assert_eq!(got, [added("top.txt"), added("sub\\new.txt")]);
    let (_, got) = wait_notify(&mut flat, mf, 4096).await;
    assert_eq!(got, [added("top.txt")]);
}
