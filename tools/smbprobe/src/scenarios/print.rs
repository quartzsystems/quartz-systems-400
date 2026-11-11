// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Print shares: a document written to one, the same through spoolss as
//! Windows prints (point and print), and the `print$` driver store.

use std::sync::Arc;

use smb_server::ntstatus::STATUS_SUCCESS;
use smb_server::wire::messages::{FileId, TreeConnectResponse};

use super::Done;
use crate::conn::{self, SMB302, Target, expect};
use crate::report::status;
use crate::smbc::client::{
    Client, Create, FILE_CREATE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_OVERWRITE_IF,
    SHARE_ALL,
};
use crate::smbc::rpc::{NDR, ack_results, bind_pdu};

/// The winspool interface, version 1.0.
const SPOOLSS: [u8; 20] = [
    0x78, 0x56, 0x34, 0x12, 0x34, 0x12, 0xcd, 0xab, 0xef, 0x00, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
    1, 0, 0, 0,
];
/// `RpcOpenPrinterEx`, `RpcGetPrinter`, `RpcGetPrinterDriver2`,
/// `RpcStartDocPrinter`, `RpcWritePrinter`, `RpcEndDocPrinter`,
/// `RpcClosePrinter`.
const OPEN_PRINTER_EX: u16 = 69;
const GET_PRINTER: u16 = 8;
const GET_PRINTER_DRIVER2: u16 = 53;
const START_DOC: u16 = 17;
const WRITE_PRINTER: u16 = 19;
const END_DOC: u16 = 23;
const CLOSE_PRINTER: u16 = 29;
/// `PRINTER_ACCESS_USE`.
const PRINTER_ACCESS_USE: u32 = 0x8;

/// What this run's documents hold: the harness finds them by it.
pub fn document(t: &Target, how: &str) -> Vec<u8> {
    format!("SMBPROBE {how} {}\r\n\x0c", t.run).into_bytes()
}

/// The print share's client from a machine of its own.
async fn print_client(t: &Target, share: &str) -> Result<Client, String> {
    let (mut s, l) = conn::session(t, t.machine(), SMB302, &t.user, &t.password, 0).await?;
    let (st, tree, kind) = conn::tree(t, &mut s, l.session_id, share).await;
    expect("tree connect to the print share", st, STATUS_SUCCESS)?;
    if kind != TreeConnectResponse::SHARE_TYPE_PRINT {
        return Err(format!("{share} answered share type {kind}, not print"));
    }
    Ok(Client::from_parts(s, l.session_id, tree))
}

/// CREATE, WRITE and CLOSE on the print share: a spooled file on its
/// output queue (the harness checks owner and data).
pub async fn print_write(t: Arc<Target>) -> Done {
    let share = t.opt.print_share.clone().unwrap_or_default();
    let mut c = print_client(&t, &share).await?;
    let id = c
        .create(Create {
            name: "smbprobe document",
            access: FILE_GENERIC_WRITE,
            disposition: FILE_OVERWRITE_IF,
            ..Create::default()
        })
        .await
        .map_err(|e| format!("CREATE on the print share: {}", status(e)))?
        .file_id;
    let data = document(&t, "PRINT-WRITE");
    let (head, tail) = data.split_at(9);
    c.write(id, 0, head)
        .await
        .map_err(|e| format!("WRITE: {}", status(e)))?;
    c.write(id, head.len() as u64, tail)
        .await
        .map_err(|e| format!("WRITE: {}", status(e)))?;
    let read = c.read(id, 0, 16).await;
    if read.is_ok() {
        return Err("a print file was readable".into());
    }
    expect("CLOSE", c.close_with(id, 0).await.0, STATUS_SUCCESS)?;
    Ok(vec![format!(
        "data: {}",
        String::from_utf8_lossy(&data).trim_end()
    )])
}

/// NDR as a client marshals it.
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
        let n = u32::try_from(units.len()).unwrap_or(0);
        self.u32(n).u32(0).u32(n);
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
        self.u32(u32::try_from(b.len()).unwrap_or(0));
        self.0.extend_from_slice(b);
        self
    }
    fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

fn dword(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4)
        .map_or(0, |d| u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
}

/// The string an INFO buffer's offset at `at` points to.
fn info_string(b: &[u8], at: usize) -> String {
    let from = dword(b, at) as usize;
    let units: Vec<u16> = b
        .get(from..)
        .unwrap_or_default()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// The INFO buffer of a GetPrinter-style answer, or its error.
fn info_buffer(out: &[u8], what: &str) -> Result<Vec<u8>, String> {
    let err = dword(out, out.len().saturating_sub(4));
    if dword(out, 0) == 0 || err != 0 {
        return Err(format!("{what}: error {err}"));
    }
    let len = dword(out, 4) as usize;
    out.get(8..8 + len)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| format!("{what}: a short answer"))
}

/// One call on the spoolss pipe; the stub, or why not.
async fn call(
    c: &mut Client,
    pipe: FileId,
    id: u32,
    op: u16,
    stub: &[u8],
) -> Result<Vec<u8>, String> {
    c.call(pipe, id, op, stub)
        .await
        .map_err(|f| format!("opnum {op}: fault 0x{f:08X}"))
}

/// `RpcGetPrinter` level 2 and `RpcGetPrinterDriver2` level 3 for x64
/// on the open printer `h`, into `lines`.
async fn describe(
    c: &mut Client,
    pipe: FileId,
    h: &[u8],
    share: &str,
    lines: &mut Vec<String>,
) -> Result<(), String> {
    let mut s = Stub::default();
    s.raw(h).u32(2).ptr(true).bytes(&[0; 8192]).u32(8192);
    let info = info_buffer(
        &call(c, pipe, 3, GET_PRINTER, &s.take()).await?,
        "RpcGetPrinter",
    )?;
    let (name, port, driver) = (
        info_string(&info, 8),
        info_string(&info, 12),
        info_string(&info, 16),
    );
    if !name.eq_ignore_ascii_case(share) {
        return Err(format!("RpcGetPrinter 2: share name {name:?}"));
    }
    lines.push(format!("printer: {name}, port {port}, driver {driver}"));

    let mut s = Stub::default();
    s.raw(h)
        .ptr(true)
        .string("Windows x64")
        .u32(3)
        .ptr(true)
        .bytes(&[0; 8192])
        .u32(8192)
        .u32(3)
        .u32(0);
    let out = call(c, pipe, 4, GET_PRINTER_DRIVER2, &s.take()).await?;
    let d = info_buffer(&out, "RpcGetPrinterDriver2")?;
    let (dname, path) = (info_string(&d, 4), info_string(&d, 12));
    if dname != driver {
        return Err(format!(
            "RpcGetPrinterDriver2: driver {dname:?}, not {driver:?}"
        ));
    }
    lines.push(format!("driver x64: {dname}, file {path:?}"));
    Ok(())
}

/// Point and print as Windows does it: `\pipe\spoolss` bound over NDR 2.0,
/// `RpcOpenPrinterEx` of the share, `RpcGetPrinter` level 2 (the share,
/// the output queue as the port, the driver), `RpcGetPrinterDriver2`
/// level 3 for x64, then a document: `RpcStartDocPrinter`,
/// `RpcWritePrinter`, `RpcEndDocPrinter`, `RpcClosePrinter` (the harness
/// finds the spooled file with these bytes).
pub async fn spoolss(t: Arc<Target>) -> Done {
    let share = t.opt.print_share.clone().unwrap_or_default();
    let user = (t.user.as_str(), t.password.as_str());
    let mut c = conn::client_as(&t, t.machine(), SMB302, user, "IPC$").await?;
    let pipe = c
        .open_pipe("spoolss")
        .await
        .map_err(|e| format!(r"\pipe\spoolss: {}", status(e)))?
        .file_id;
    expect(
        "bind",
        c.pipe_write(pipe, &bind_pdu(1, 4280, &[(0, SPOOLSS, NDR)]))
            .await,
        STATUS_SUCCESS,
    )?;
    let (_, ack) = c.pipe_read(pipe, 4280).await;
    if ack_results(&ack) != [(0, 0)] {
        return Err(format!("bind: {:?}", ack_results(&ack)));
    }
    let mut s = Stub::default();
    s.ptr(true)
        .string(&t.unc(&share))
        .ptr(false)
        .u32(0)
        .ptr(false)
        .u32(PRINTER_ACCESS_USE)
        .u32(1)
        .u32(1)
        .ptr(false);
    let out = call(&mut c, pipe, 2, OPEN_PRINTER_EX, &s.take()).await?;
    if dword(&out, 20) != 0 || out.len() < 24 {
        return Err(format!("RpcOpenPrinterEx: error {}", dword(&out, 20)));
    }
    let h = out[..20].to_vec();
    let mut lines = Vec::new();

    describe(&mut c, pipe, &h, &share, &mut lines).await?;

    let mut s = Stub::default();
    s.raw(&h)
        .u32(1)
        .u32(1)
        .ptr(true)
        .ptr(true)
        .ptr(false)
        .ptr(false)
        .string("smbprobe spoolss");
    let out = call(&mut c, pipe, 5, START_DOC, &s.take()).await?;
    if dword(&out, 4) != 0 {
        return Err(format!("RpcStartDocPrinter: error {}", dword(&out, 4)));
    }
    let job = dword(&out, 0);
    let data = document(&t, "SPOOLSS");
    let n = u32::try_from(data.len()).unwrap_or(0);
    let mut s = Stub::default();
    s.raw(&h).bytes(&data).u32(n);
    let out = call(&mut c, pipe, 6, WRITE_PRINTER, &s.take()).await?;
    if (dword(&out, 0), dword(&out, 4)) != (n, 0) {
        return Err(format!(
            "RpcWritePrinter: {} written, error {}",
            dword(&out, 0),
            dword(&out, 4)
        ));
    }
    let mut s = Stub::default();
    s.raw(&h);
    let out = call(&mut c, pipe, 7, END_DOC, &s.take()).await?;
    if dword(&out, 0) != 0 {
        return Err(format!("RpcEndDocPrinter: error {}", dword(&out, 0)));
    }
    let mut s = Stub::default();
    s.raw(&h);
    let out = call(&mut c, pipe, 8, CLOSE_PRINTER, &s.take()).await?;
    if dword(&out, out.len().saturating_sub(4)) != 0 {
        return Err(format!(
            "RpcClosePrinter: error {}",
            dword(&out, out.len() - 4)
        ));
    }
    lines.push(format!(
        "job {job}; data: {}",
        String::from_utf8_lossy(&data).trim_end()
    ));
    Ok(lines)
}

/// The read-only `print$` share serves the driver store: the file
/// `--driver-file` names reads as given, and a new file is refused.
pub async fn print_dollar(t: Arc<Target>) -> Done {
    let (path, want) = t.opt.driver_file.clone().unwrap_or_default();
    let path = path.replace('/', "\\");
    let user = (t.user.as_str(), t.password.as_str());
    let mut c = conn::client_as(&t, t.machine(), SMB302, user, "print$").await?;
    let dir = c.open_dir("").await;
    let top = c
        .list(dir, 0, "*")
        .await
        .map_err(|e| format!("listing: {}", status(e)))?;
    c.close(dir).await;
    let id = c
        .open(&path, FILE_GENERIC_READ, SHARE_ALL)
        .await
        .map_err(|e| format!("{path}: {}", status(e)))?;
    let data = c
        .read(id, 0, 4096)
        .await
        .map_err(|e| format!("read {path}: {}", status(e)))?;
    c.close(id).await;
    if data != want {
        return Err(format!("{path} reads {:?}", String::from_utf8_lossy(&data)));
    }
    let made = c
        .create(Create {
            name: r"x64\3\smbprobe.dll",
            access: FILE_GENERIC_WRITE,
            disposition: FILE_CREATE,
            ..Create::default()
        })
        .await;
    if let Ok(r) = made {
        c.close(r.file_id).await;
        return Err("print$ took a new file".into());
    }
    Ok(vec![
        format!("listing: {}", top.join(", ")),
        format!("{path}: {} bytes as stored; a new file refused", data.len()),
    ])
}
