//! QS/400's 8.3 short names (`QS400-PATCHES.md`): the backend's short
//! names in FileBothDirectoryInformation and FileIdBothDirectoryInformation
//! listings and in FileAlternateNameInformation, and the generator a
//! backend without stored short names uses.

#![cfg(feature = "localfs")]

mod common;

use async_trait::async_trait;
use common::client::*;
use smb_server::short_name;
use smb_server::{
    BackendCapabilities, DirEntry, FileInfo, FileTimes, Handle, OpenAction, OpenOptions, Share,
    ShareBackend, SmbError, SmbPath, SmbResult, SmbServer,
};

const LONG: &str = "Long File Name.txt";
const PLAIN: &str = "README.TXT";
const FILE_BOTH_DIRECTORY_INFORMATION: u8 = 0x03;
const STATUS_OBJECT_NAME_NOT_FOUND: u32 = 0xC000_0034;

/// A share root holding two files: one long name (file 7) and one 8.3.
struct Two;

/// An open of the root (`None`) or of one of its files.
struct TwoHandle(Option<&'static str>);

fn names() -> Vec<(&'static str, u64)> {
    vec![(LONG, 7), (PLAIN, 8)]
}

fn info(name: &str, id: u64) -> FileInfo {
    let mut i = FileInfo::new(name);
    i.file_index = id;
    i
}

#[async_trait]
impl ShareBackend for Two {
    async fn open(&self, p: &SmbPath, _: OpenOptions) -> SmbResult<(Box<dyn Handle>, OpenAction)> {
        let Some(want) = p.components().first() else {
            return Ok((Box::new(TwoHandle(None)), OpenAction::Opened));
        };
        let entries = names();
        // The long name, or a short name of one.
        let at = entries
            .iter()
            .position(|(n, _)| n.eq_ignore_ascii_case(want))
            .or_else(|| short_name::resolve(&entries, false, want))
            .ok_or(SmbError::NotFound)?;
        Ok((Box::new(TwoHandle(Some(entries[at].0))), OpenAction::Opened))
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
impl Handle for TwoHandle {
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
        Ok(match self.0 {
            None => {
                let mut i = info("", 1);
                i.is_directory = true;
                i
            }
            Some(n) => info(n, names().iter().find(|e| e.0 == n).unwrap().1),
        })
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
        let entries = names();
        let short = short_name::assign(&entries, false);
        Ok(entries
            .iter()
            .zip(short)
            .map(|(&(n, id), s)| {
                let mut i = info(n, id);
                i.short_name = s;
                DirEntry { info: i }
            })
            .collect())
    }
    async fn close(self: Box<Self>) -> SmbResult<()> {
        Ok(())
    }
    async fn short_name(&self) -> SmbResult<Option<String>> {
        let Some(n) = self.0 else { return Ok(None) };
        let entries = names();
        let at = entries.iter().position(|e| e.0 == n).unwrap();
        Ok(short_name::assign(&entries, false)[at].clone())
    }
}

/// (name, short name) of each entry of a Both (`id` false) or IdBoth
/// listing.
fn entries(mut buf: &[u8], id: bool) -> Vec<(String, String)> {
    let name_at = if id { 104 } else { 94 };
    let utf16 = |b: &[u8]| {
        let u: Vec<u16> = b
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&u).unwrap()
    };
    let mut out = Vec::new();
    while buf.len() >= name_at {
        let next = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
        let len = u32::from_le_bytes(buf[60..64].try_into().unwrap()) as usize;
        let short_len = usize::from(buf[68]);
        assert!(short_len <= 24 && buf[70 + short_len..94].iter().all(|&b| b == 0));
        out.push((
            utf16(&buf[name_at..name_at + len]),
            utf16(&buf[70..70 + short_len]),
        ));
        if next == 0 {
            break;
        }
        buf = &buf[next..];
    }
    out
}

#[tokio::test]
async fn listings_and_the_alternate_name_carry_short_names() {
    let server = Server::start(SmbServer::builder().share(Share::new("share", Two).public())).await;
    let mut c = server.client().await;
    let short = short_name::assign(&names(), false)[0].clone().unwrap();
    assert!(
        short.starts_with("LO") && short.ends_with("~1.TXT"),
        "{short}"
    );
    for (class, id) in [
        (FILE_BOTH_DIRECTORY_INFORMATION, false),
        (FILE_ID_BOTH_DIRECTORY_INFORMATION, true),
    ] {
        let dir = c.open_dir("").await;
        let (status, buf) = c.query_directory(dir, class, 0, "*", 65536).await;
        assert_eq!(status, 0);
        let got = entries(&buf, id);
        assert!(got.contains(&(LONG.to_owned(), short.clone())), "{got:?}");
        assert!(got.contains(&(PLAIN.to_owned(), String::new())), "{got:?}");
        assert!(got.contains(&(".".to_owned(), String::new())), "{got:?}");
        c.close(dir).await;
    }

    // FileAlternateNameInformation: FileNameLength and the name.
    let f = c.open(LONG, 0x0012_0089, 7).await.unwrap();
    let buf = c
        .query_file_info(f, FILE_ALTERNATE_NAME_INFORMATION)
        .await
        .unwrap();
    let len = u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize;
    let u: Vec<u16> = buf[4..4 + len]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    assert_eq!(String::from_utf16(&u).unwrap(), short);
    c.close(f).await;

    // An 8.3 name and the root have none.
    let f = c.open(PLAIN, 0x0012_0089, 7).await.unwrap();
    assert_eq!(
        c.query_file_info(f, FILE_ALTERNATE_NAME_INFORMATION).await,
        Err(STATUS_OBJECT_NAME_NOT_FOUND)
    );
    c.close(f).await;

    // Opening by the short name reaches the long-named file.
    let f = c.open(&short.to_lowercase(), 0x0012_0089, 7).await.unwrap();
    let buf = c
        .query_file_info(f, FILE_ALTERNATE_NAME_INFORMATION)
        .await
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(buf[0..4].try_into().unwrap()) as usize,
        short.len() * 2
    );
    c.close(f).await;
}

#[tokio::test]
async fn a_short_alternate_name_buffer_is_cut_with_overflow() {
    let server = Server::start(SmbServer::builder().share(Share::new("share", Two).public())).await;
    let mut c = server.client().await;
    let f = c.open(LONG, 0x0012_0089, 7).await.unwrap();
    let (status, buf) = c
        .query_info(f, INFO_FILE, FILE_ALTERNATE_NAME_INFORMATION, 10)
        .await;
    assert_eq!(status, 0x8000_0005);
    assert_eq!(buf.len(), 10);
    assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), 24);
}
