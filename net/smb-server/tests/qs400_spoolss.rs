//! QS/400's spoolss pipe (`QS400-PATCHES.md`): point and print over a
//! real connection, from opening the printer to the bytes reaching the
//! print service.

#![cfg(feature = "localfs")]

mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use common::client::Server;
use common::durable::SMB21;
use common::rpc::*;
use smb_server::Identity;
use smb_server::ntstatus::{STATUS_OBJECT_NAME_NOT_FOUND, STATUS_SUCCESS};
use smb_server::print::{DocInfo, JobInfo, PrintError, PrintJob, PrintService, PrinterInfo};
use smb_server::wire::messages::FileId;
use smb_server::{Share, SmbServer};
use tempfile::TempDir;

/// winspool 1.0 (MS-RPRN 2.1), as on the wire.
const SPOOLSS: [u8; 20] = [
    0x78, 0x56, 0x34, 0x12, 0x34, 0x12, 0xcd, 0xab, 0xef, 0x00, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
    1, 0, 0, 0,
];

const OPEN_PRINTER_EX: u16 = 69;
const GET_PRINTER: u16 = 8;
const GET_PRINTER_DRIVER_2: u16 = 53;
const START_DOC_PRINTER: u16 = 17;
const WRITE_PRINTER: u16 = 19;
const END_DOC_PRINTER: u16 = 23;
const CLOSE_PRINTER: u16 = 29;
const ADD_PRINTER_DRIVER_EX: u16 = 89;

/// Jobs as the service got them: share, user, document, bytes, ended.
type Jobs = Arc<Mutex<Vec<(String, String, String, Vec<u8>, bool)>>>;

/// One printer, `OFFICE`, for signed-on users.
#[derive(Clone, Default)]
struct OnePrinter {
    jobs: Jobs,
}

#[async_trait]
impl PrintService for OnePrinter {
    async fn printers(&self, who: &Identity) -> Vec<PrinterInfo> {
        match who {
            Identity::Anonymous => Vec::new(),
            Identity::User { .. } => vec![PrinterInfo {
                share: "OFFICE".into(),
                driver: "Microsoft PCL6 Class Driver".into(),
                comment: "Office printer".into(),
                location: String::new(),
                port: "QGPL/QPRINT".into(),
                default_form: "Letter".into(),
                jobs: 0,
                driver_files: Vec::new(),
            }],
        }
    }

    async fn start_job(
        &self,
        who: &Identity,
        share: &str,
        doc: &DocInfo,
    ) -> Result<Box<dyn PrintJob>, PrintError> {
        let Identity::User { user, .. } = who else {
            return Err(PrintError::AccessDenied);
        };
        let mut jobs = self.jobs.lock().unwrap();
        jobs.push((
            share.into(),
            user.clone(),
            doc.document.clone(),
            Vec::new(),
            false,
        ));
        Ok(Box::new(Job {
            index: jobs.len() - 1,
            jobs: Arc::clone(&self.jobs),
        }))
    }

    async fn jobs(&self, _: &Identity, _: &str) -> Vec<JobInfo> {
        Vec::new()
    }
}

struct Job {
    index: usize,
    jobs: Jobs,
}

#[async_trait]
impl PrintJob for Job {
    fn id(&self) -> u32 {
        self.index as u32 + 1
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), PrintError> {
        self.jobs.lock().unwrap()[self.index]
            .3
            .extend_from_slice(data);
        Ok(())
    }

    async fn end(self: Box<Self>, _: u32) -> Result<(), PrintError> {
        self.jobs.lock().unwrap()[self.index].4 = true;
        Ok(())
    }
}

async fn server(print: Option<OnePrinter>) -> (Server, TempDir) {
    let td = tempfile::tempdir().expect("tempdir");
    let mut builder = SmbServer::builder()
        .user("alice", "password")
        .share(Share::new("public", common::client::local(td.path())).public());
    if let Some(print) = print {
        builder = builder.print_service(print);
    }
    (Server::start(builder).await, td)
}

/// NDR as a client marshals it.
#[derive(Default)]
struct Stub {
    buf: Vec<u8>,
    next: u32,
}

impl Stub {
    fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.resize(self.buf.len().next_multiple_of(4), 0);
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    fn pointer(&mut self, present: bool) -> &mut Self {
        if present {
            self.next += 4;
            self.u32(0x0002_0000 + self.next)
        } else {
            self.u32(0)
        }
    }

    fn string(&mut self, text: &str) -> &mut Self {
        let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
        self.u32(units.len() as u32).u32(0).u32(units.len() as u32);
        for u in units {
            self.buf.extend_from_slice(&u.to_le_bytes());
        }
        self
    }

    fn handle(&mut self, h: &[u8]) -> &mut Self {
        self.buf.resize(self.buf.len().next_multiple_of(4), 0);
        self.buf.extend_from_slice(h);
        self
    }

    fn bytes(&mut self, data: &[u8]) -> &mut Self {
        self.u32(data.len() as u32);
        self.buf.extend_from_slice(data);
        self
    }

    fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }
}

fn dword(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
}

/// The string an INFO buffer's offset at `at` points to.
fn info_string(buf: &[u8], at: usize) -> String {
    let from = dword(buf, at) as usize;
    let units: Vec<u16> = buf[from..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16(&units).unwrap()
}

/// The INFO buffer of a GetPrinter-style answer (pointer, array), after
/// which come `tail` DWORDs, the last the status.
fn info_of(stub: &[u8], tail: usize) -> (Vec<u8>, Vec<u32>) {
    assert_ne!(dword(stub, 0), 0, "a buffer");
    let len = dword(stub, 4) as usize;
    let buf = stub[8..8 + len].to_vec();
    let at = (8 + len).next_multiple_of(4);
    let rest = (0..tail).map(|i| dword(stub, at + 4 * i)).collect();
    assert_eq!(stub.len(), at + 4 * tail);
    (buf, rest)
}

/// Opens spoolss on a client signed on as alice and binds it.
async fn bound(server: &Server) -> (common::client::Client, FileId) {
    let mut c = ipc_client(server.addr, SMB21, Some("alice")).await;
    let file_id = c.open_pipe("spoolss").await.expect("open spoolss").file_id;
    let bind = bind_pdu(1, 4280, &[(0, SPOOLSS, NDR)]);
    assert_eq!(c.pipe_write(file_id, &bind).await, STATUS_SUCCESS);
    let (status, ack) = c.pipe_read(file_id, 4280).await;
    assert_eq!(status, STATUS_SUCCESS);
    assert_pdu(&ack, PTYPE_BIND_ACK);
    assert_eq!(ack_results(&ack), [(0, 0)]);
    (c, file_id)
}

/// A call whose request goes in fragments of at most 4000 stub bytes.
async fn call_in_fragments(
    c: &mut common::client::Client,
    file_id: FileId,
    call_id: u32,
    opnum: u16,
    stub: &[u8],
) -> Vec<u8> {
    let chunks: Vec<&[u8]> = stub.chunks(4000).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let mut flags = 0;
        if i == 0 {
            flags |= FIRST;
        }
        if i == chunks.len() - 1 {
            flags |= LAST;
        }
        let pdu = request_pdu(call_id, flags, 0, opnum, chunk);
        assert_eq!(c.pipe_write(file_id, &pdu).await, STATUS_SUCCESS);
    }
    let (status, pdu) = c.pipe_read(file_id, 4280).await;
    assert_eq!(status, STATUS_SUCCESS);
    assert_pdu(&pdu, PTYPE_RESPONSE);
    assert_eq!(pfc_flags(&pdu) & LAST, LAST);
    pdu[24..].to_vec()
}

#[tokio::test]
async fn spoolss_exists_only_with_a_print_service() {
    let (server, _td) = server(None).await;
    let mut c = ipc_client(server.addr, SMB21, Some("alice")).await;
    assert_eq!(
        c.open_pipe("spoolss").await.err(),
        Some(STATUS_OBJECT_NAME_NOT_FOUND)
    );
    let (server, _td) = server_with_print().await;
    let mut c = ipc_client(server.addr, SMB21, Some("alice")).await;
    for name in ["spoolss", "SPOOLSS", "srvsvc"] {
        c.open_pipe(name).await.expect(name);
    }
}

async fn server_with_print() -> (Server, TempDir) {
    server(Some(OnePrinter::default())).await
}

#[tokio::test]
async fn a_client_connects_the_printer_and_prints_through_it() {
    let print = OnePrinter::default();
    let (server, _td) = server(Some(print.clone())).await;
    let (mut c, pipe) = bound(&server).await;

    // RpcOpenPrinterEx \\127.0.0.1\OFFICE for PRINTER_ACCESS_USE.
    let mut s = Stub::default();
    s.pointer(true)
        .string("\\\\127.0.0.1\\OFFICE")
        .pointer(false);
    s.u32(0).pointer(false).u32(0x8);
    s.u32(1).u32(1).pointer(true);
    s.u32(28)
        .pointer(true)
        .pointer(true)
        .u32(22631)
        .u32(10)
        .u32(0);
    s.buf.extend_from_slice(&9u16.to_le_bytes());
    s.string("\\\\DESKTOP-1").string("alice");
    let out = c.call(pipe, 2, OPEN_PRINTER_EX, &s.take()).await.unwrap();
    assert_eq!(out.len(), 24);
    assert_eq!(dword(&out, 20), 0);
    let handle = out[..20].to_vec();
    assert_ne!(handle, [0; 20]);

    // RpcGetPrinter level 2: the size, then the printer.
    let get = |len: Option<u32>| {
        let mut s = Stub::default();
        s.handle(&handle).u32(2).pointer(len.is_some());
        if let Some(len) = len {
            s.bytes(&vec![0; len as usize]);
        }
        s.u32(len.unwrap_or(0));
        s.take()
    };
    let out = c.call(pipe, 3, GET_PRINTER, &get(None)).await.unwrap();
    assert_eq!(dword(&out, 0), 0, "no buffer");
    let needed = dword(&out, 4);
    assert_eq!(dword(&out, 8), 122, "ERROR_INSUFFICIENT_BUFFER");
    let out = c
        .call(pipe, 4, GET_PRINTER, &get(Some(needed)))
        .await
        .unwrap();
    let (info, rest) = info_of(&out, 2);
    assert_eq!(rest, [needed, 0]);
    assert!(info_string(&info, 4).ends_with("\\OFFICE"));
    assert_eq!(info_string(&info, 16), "Microsoft PCL6 Class Driver");

    // RpcGetPrinterDriver2 level 3 for Windows x64, client 3.0.
    let mut s = Stub::default();
    s.handle(&handle).pointer(true).string("Windows x64").u32(3);
    s.pointer(true).bytes(&[0; 2048]).u32(2048).u32(3).u32(0);
    let out = c
        .call(pipe, 5, GET_PRINTER_DRIVER_2, &s.take())
        .await
        .unwrap();
    let (info, rest) = info_of(&out, 4);
    assert_eq!(rest[1..], [3, 0, 0], "server version 3.0, success");
    assert_eq!(dword(&info, 0), 3);
    assert_eq!(info_string(&info, 4), "Microsoft PCL6 Class Driver");

    // RpcStartDocPrinter, a write bigger than a fragment, RpcEndDocPrinter.
    let mut s = Stub::default();
    s.handle(&handle).u32(1).u32(1).pointer(true);
    s.pointer(true).pointer(false).pointer(true);
    s.string("Test Page").string("RAW");
    let out = c.call(pipe, 6, START_DOC_PRINTER, &s.take()).await.unwrap();
    assert_eq!((dword(&out, 0), dword(&out, 4)), (1, 0), "job 1");
    let page: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
    let mut s = Stub::default();
    s.handle(&handle).bytes(&page).u32(page.len() as u32);
    let out = call_in_fragments(&mut c, pipe, 7, WRITE_PRINTER, &s.take()).await;
    assert_eq!((dword(&out, 0), dword(&out, 4)), (20_000, 0));
    let mut s = Stub::default();
    s.handle(&handle);
    let out = c.call(pipe, 8, END_DOC_PRINTER, &s.take()).await.unwrap();
    assert_eq!(out, [0; 4]);

    // Administration is refused; RpcClosePrinter zeroes the handle.
    let mut s = Stub::default();
    s.pointer(false).u32(3);
    assert_eq!(
        c.call(pipe, 9, ADD_PRINTER_DRIVER_EX, &s.take()).await,
        Err(5)
    );
    let mut s = Stub::default();
    s.handle(&handle);
    let out = c.call(pipe, 10, CLOSE_PRINTER, &s.take()).await.unwrap();
    assert_eq!(out, [0; 24]);

    let jobs = print.jobs.lock().unwrap();
    assert_eq!(jobs.len(), 1);
    let (share, user, doc, data, ended) = &jobs[0];
    assert_eq!(
        (share.as_str(), user.as_str(), doc.as_str()),
        ("OFFICE", "alice", "Test Page")
    );
    assert_eq!(data, &page);
    assert!(ended);
}

#[tokio::test]
async fn an_anonymous_session_opens_no_printer() {
    let (server, _td) = server_with_print().await;
    let mut c = ipc_client(server.addr, SMB21, None).await;
    let pipe = c.open_pipe("spoolss").await.expect("open").file_id;
    let bind = bind_pdu(1, 4280, &[(0, SPOOLSS, NDR)]);
    c.pipe_write(pipe, &bind).await;
    c.pipe_read(pipe, 4280).await;
    let mut s = Stub::default();
    s.pointer(true).string("OFFICE").pointer(false);
    s.u32(0)
        .pointer(false)
        .u32(0x8)
        .u32(1)
        .u32(1)
        .pointer(false);
    let out = c.call(pipe, 2, OPEN_PRINTER_EX, &s.take()).await.unwrap();
    assert_eq!(&out[..20], &[0; 20]);
    assert_eq!(dword(&out, 20), 1801, "ERROR_INVALID_PRINTER_NAME");
}
