//! The handles one spoolss pipe has open (MS-RPRN 3.1.4.2.14,
//! RpcOpenPrinterEx; 3.1.4.2.9, RpcClosePrinter): the server or one
//! printer, the access granted, and the document being printed.

use std::collections::HashMap;

use super::{Ctx, bad_stub, codes, string_of, u32_of, unique_string_of};
use crate::print::PrintJob;
use crate::rpc::association::Fault;
use crate::rpc::ndr::{NdrReader, NdrWriter};

/// Handles one pipe may hold open.
pub(crate) const MAX_HANDLES: usize = 64;

/// Access rights (MS-RPRN 2.2.3.1).
pub(crate) const SERVER_ACCESS_ENUMERATE: u32 = 0x0000_0002;
pub(crate) const PRINTER_ACCESS_USE: u32 = 0x0000_0008;
const READ_CONTROL: u32 = 0x0002_0000;
const SYNCHRONIZE: u32 = 0x0010_0000;
const MAXIMUM_ALLOWED: u32 = 0x0200_0000;
const GENERIC_ALL: u32 = 0x1000_0000;
const GENERIC_EXECUTE: u32 = 0x2000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const GENERIC_READ: u32 = 0x8000_0000;

/// Largest DEVMODE an open may carry; it is not kept.
const MAX_DEVMODE: usize = 64 << 10;

/// What a handle names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// The print server, `\\NAME`.
    Server,
    /// A printer, by its share name.
    Printer(String),
}

/// A document being printed through a handle.
pub(crate) struct Doc {
    pub job: Box<dyn PrintJob>,
    /// Pages ended so far.
    pub pages: u32,
    /// A page is started and not ended.
    pub in_page: bool,
}

pub(crate) struct Handle {
    pub target: Target,
    pub granted: u32,
    /// The client machine its open named, or empty.
    pub machine: String,
    pub doc: Option<Doc>,
}

/// The open handles of one pipe. Dropping it abandons every document
/// still open.
#[derive(Default)]
pub(crate) struct Session {
    handles: HashMap<[u8; 20], Handle>,
}

impl Session {
    /// Reads a handle argument: one this pipe opened, or a fault.
    pub(crate) fn handle(&self, r: &mut NdrReader<'_>) -> Result<&Handle, Fault> {
        let id = r.handle().ok_or_else(bad_stub)?;
        self.handles.get(&id).ok_or(Fault {
            status: super::CONTEXT_MISMATCH,
            did_not_execute: true,
        })
    }

    /// As [`Session::handle`], to change it.
    pub(crate) fn handle_mut(&mut self, r: &mut NdrReader<'_>) -> Result<&mut Handle, Fault> {
        let id = r.handle().ok_or_else(bad_stub)?;
        self.handles.get_mut(&id).ok_or(Fault {
            status: super::CONTEXT_MISMATCH,
            did_not_execute: true,
        })
    }

    /// Handles open now.
    #[cfg(test)]
    pub(crate) fn open_handles(&self) -> usize {
        self.handles.len()
    }

    /// RpcOpenPrinter and RpcOpenPrinterEx (`ex`, with client info).
    pub(crate) async fn open(
        &mut self,
        ctx: &Ctx<'_>,
        r: &mut NdrReader<'_>,
        ex: bool,
    ) -> Result<Vec<u8>, Fault> {
        let name = unique_string_of(r)?;
        let datatype = unique_string_of(r)?;
        // DEVMODE_CONTAINER: cbBuf, then the DEVMODE.
        u32_of(r)?;
        if r.pointer().ok_or_else(bad_stub)? {
            r.bytes(MAX_DEVMODE).ok_or_else(bad_stub)?;
        }
        let access = u32_of(r)?;
        let machine = if ex {
            client_machine(r)?
        } else {
            String::new()
        };
        let status = match self.target(ctx, name.as_deref(), datatype.as_deref()).await {
            Err(status) => Err(status),
            Ok(target) => match grant(&target, access) {
                None => Err(codes::ERROR_ACCESS_DENIED),
                Some(_) if self.handles.len() >= MAX_HANDLES => Err(codes::ERROR_NOT_ENOUGH_MEMORY),
                Some(granted) => Ok(Handle {
                    target,
                    granted,
                    machine,
                    doc: None,
                }),
            },
        };
        let mut w = NdrWriter::new();
        match status {
            Ok(handle) => {
                let id = new_handle_id();
                self.handles.insert(id, handle);
                w.handle(&id);
                w.u32(codes::ERROR_SUCCESS);
            }
            Err(status) => {
                w.handle(&[0; 20]);
                w.u32(status);
            }
        }
        Ok(w.finish())
    }

    /// What an open names: the server for no name, `\\NAME` or `\\NAME\`;
    /// the printer for `\\NAME\SHARE` or `SHARE`. The server part is not
    /// checked (the client reached this server by it). A name with a
    /// `,` suffix (`,Job n`, `,XcvPort`) is not one this server opens.
    async fn target(
        &self,
        ctx: &Ctx<'_>,
        name: Option<&str>,
        datatype: Option<&str>,
    ) -> Result<Target, u32> {
        if datatype.is_some_and(|d| !d.eq_ignore_ascii_case(super::info::RAW)) {
            return Err(codes::ERROR_INVALID_DATATYPE);
        }
        let share = match name.unwrap_or("") {
            "" => return Ok(Target::Server),
            name => match name.strip_prefix("\\\\") {
                Some(rest) => match rest.split_once('\\') {
                    None | Some((_, "")) => return Ok(Target::Server),
                    Some((_, share)) => share,
                },
                None => name,
            },
        };
        if share.contains([',', '\\']) {
            return Err(codes::ERROR_INVALID_PRINTER_NAME);
        }
        match ctx.printer(share).await {
            Some(p) => Ok(Target::Printer(p.share)),
            None => Err(codes::ERROR_INVALID_PRINTER_NAME),
        }
    }

    /// RpcClosePrinter: the handle goes, with its document ended.
    pub(crate) async fn close(&mut self, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
        let id = r.handle().ok_or_else(bad_stub)?;
        let Some(handle) = self.handles.remove(&id) else {
            return Err(Fault {
                status: super::CONTEXT_MISMATCH,
                did_not_execute: true,
            });
        };
        if let Some(doc) = handle.doc
            && let Err(e) = doc.job.end(doc.pages).await
        {
            tracing::debug!(error = %e, "job ended by close failed");
        }
        let mut w = NdrWriter::new();
        w.handle(&[0; 20]);
        w.u32(codes::ERROR_SUCCESS);
        Ok(w.finish())
    }
}

/// The access an open of `target` asking for `access` is granted, or
/// `None` when it asks for what no session here holds: administering the
/// server or a printer, deleting it, or changing its security.
fn grant(target: &Target, access: u32) -> Option<u32> {
    let allowed = match target {
        Target::Server => SERVER_ACCESS_ENUMERATE | READ_CONTROL,
        Target::Printer(_) => PRINTER_ACCESS_USE | READ_CONTROL,
    };
    if access & GENERIC_ALL != 0 {
        return None;
    }
    let mut asked = access & !(MAXIMUM_ALLOWED | SYNCHRONIZE);
    if asked & (GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE) != 0 {
        asked = (asked & !(GENERIC_READ | GENERIC_WRITE | GENERIC_EXECUTE)) | allowed;
    }
    if asked & !allowed != 0 {
        return None;
    }
    Some(allowed)
}

/// SPLCLIENT_CONTAINER (MS-RPRN 2.2.1.2.14): the machine name of level 1;
/// the other levels name none.
fn client_machine(r: &mut NdrReader<'_>) -> Result<String, Fault> {
    let level = u32_of(r)?;
    if u32_of(r)? != level {
        return Err(bad_stub());
    }
    if !r.pointer().ok_or_else(bad_stub)? {
        return Ok(String::new());
    }
    match level {
        1 => {
            // dwSize, pMachineName, pUserName, dwBuildNum, dwMajorVersion,
            // dwMinorVersion, wProcessorArchitecture.
            u32_of(r)?;
            let machine = r.pointer().ok_or_else(bad_stub)?;
            let user = r.pointer().ok_or_else(bad_stub)?;
            for _ in 0..3 {
                u32_of(r)?;
            }
            r.u16().ok_or_else(bad_stub)?;
            let name = if machine {
                string_of(r)?
            } else {
                String::new()
            };
            if user {
                string_of(r)?;
            }
            Ok(name.trim_start_matches('\\').to_string())
        }
        2 | 3 => Ok(String::new()),
        _ => Err(bad_stub()),
    }
}

/// A new context handle: attributes 0 and a random UUID.
fn new_handle_id() -> [u8; 20] {
    let mut id = [0; 20];
    id[4..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printers_grant_use_and_refuse_administration() {
        let p = Target::Printer("P".into());
        let use_ = PRINTER_ACCESS_USE | READ_CONTROL;
        for asked in [
            0,
            PRINTER_ACCESS_USE,
            MAXIMUM_ALLOWED,
            GENERIC_READ,
            GENERIC_EXECUTE,
        ] {
            assert_eq!(grant(&p, asked), Some(use_), "{asked:#x}");
        }
        // PRINTER_ALL_ACCESS, PRINTER_ACCESS_ADMINISTER, WRITE_DAC, DELETE.
        for asked in [0x000F_000C, 0x4, 0x0004_0000, 0x0001_0000, GENERIC_ALL] {
            assert_eq!(grant(&p, asked), None, "{asked:#x}");
        }
        assert_eq!(
            grant(&Target::Server, SERVER_ACCESS_ENUMERATE),
            Some(SERVER_ACCESS_ENUMERATE | READ_CONTROL)
        );
        // SERVER_ACCESS_ADMINISTER.
        assert_eq!(grant(&Target::Server, 0x1), None);
    }
}
