//! QS/400: the print spooler's calls (MS-RPRN 3.1.4) on `\pipe\spoolss`,
//! the subset Windows uses to connect a printer and print through it.
//! Administration calls fault with ERROR_ACCESS_DENIED; every other
//! operation not served faults with nca_op_rng_error, or returns
//! ERROR_NOT_SUPPORTED where its only output is the status.

pub(crate) mod data;
pub(crate) mod forms;
pub(crate) mod info;
pub(crate) mod job;
pub(crate) mod marshal;
#[cfg(any(test, fuzzing, feature = "fuzzing"))]
pub(crate) mod mem;
pub(crate) mod printer;
pub(crate) mod session;
#[cfg(test)]
mod tests;

use super::association::Fault;
use super::ndr::{NdrReader, NdrWriter};
use super::pdu::NCA_OP_RNG_ERROR;
use crate::print::{PrintService, PrinterInfo};
use crate::proto::auth::ntlm::Identity;
use crate::server::{Caller, ServerState};
pub(crate) use session::Session;

/// Win32 error codes the calls return (MS-ERREF 2.2).
pub(crate) mod codes {
    pub const ERROR_SUCCESS: u32 = 0;
    pub const ERROR_FILE_NOT_FOUND: u32 = 2;
    pub const ERROR_ACCESS_DENIED: u32 = 5;
    pub const ERROR_INVALID_HANDLE: u32 = 6;
    pub const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
    pub const ERROR_NOT_SUPPORTED: u32 = 50;
    pub const ERROR_INVALID_PARAMETER: u32 = 87;
    pub const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    pub const ERROR_INVALID_NAME: u32 = 123;
    pub const ERROR_INVALID_LEVEL: u32 = 124;
    pub const ERROR_MORE_DATA: u32 = 234;
    pub const ERROR_NO_MORE_ITEMS: u32 = 259;
    pub const ERROR_UNKNOWN_PRINTER_DRIVER: u32 = 1797;
    pub const ERROR_INVALID_PRINTER_NAME: u32 = 1801;
    pub const ERROR_INVALID_DATATYPE: u32 = 1804;
    pub const ERROR_INVALID_ENVIRONMENT: u32 = 1805;
    pub const ERROR_INVALID_FORM_NAME: u32 = 1902;
    pub const ERROR_PRINTER_DELETED: u32 = 1905;
    pub const ERROR_INVALID_PRINTER_STATE: u32 = 1906;
    pub const ERROR_SPL_NO_STARTDOC: u32 = 3003;
}

/// Operation numbers (MS-RPRN 3.1.4).
pub(crate) mod opnum {
    pub const ENUM_PRINTERS: u16 = 0;
    pub const OPEN_PRINTER: u16 = 1;
    pub const SET_JOB: u16 = 2;
    pub const GET_JOB: u16 = 3;
    pub const ENUM_JOBS: u16 = 4;
    pub const GET_PRINTER: u16 = 8;
    pub const GET_PRINTER_DRIVER: u16 = 11;
    pub const START_DOC_PRINTER: u16 = 17;
    pub const START_PAGE_PRINTER: u16 = 18;
    pub const WRITE_PRINTER: u16 = 19;
    pub const END_PAGE_PRINTER: u16 = 20;
    pub const ABORT_PRINTER: u16 = 21;
    pub const END_DOC_PRINTER: u16 = 23;
    pub const SCHEDULE_JOB: u16 = 25;
    pub const GET_PRINTER_DATA: u16 = 26;
    pub const CLOSE_PRINTER: u16 = 29;
    pub const GET_FORM: u16 = 32;
    pub const ENUM_FORMS: u16 = 34;
    pub const RESET_PRINTER: u16 = 52;
    pub const GET_PRINTER_DRIVER_2: u16 = 53;
    pub const FIND_CLOSE_PRINTER_CHANGE_NOTIFICATION: u16 = 56;
    pub const REMOTE_FIND_FIRST_PRINTER_CHANGE_NOTIFICATION: u16 = 62;
    pub const REMOTE_FIND_FIRST_PRINTER_CHANGE_NOTIFICATION_EX: u16 = 65;
    pub const OPEN_PRINTER_EX: u16 = 69;
    pub const ENUM_PRINTER_DATA: u16 = 72;
    pub const GET_PRINTER_DATA_EX: u16 = 78;
    pub const ENUM_PRINTER_DATA_EX: u16 = 79;
    pub const ENUM_PRINTER_KEY: u16 = 80;
}

/// Administration operations, refused with ERROR_ACCESS_DENIED: add, set
/// and delete printers, drivers, forms, printer data, ports, monitors,
/// print processors and per-machine connections, and XcvData.
pub(crate) const ADMINISTRATION: [u16; 26] = [
    5,  // RpcAddPrinter
    6,  // RpcDeletePrinter
    7,  // RpcSetPrinter
    9,  // RpcAddPrinterDriver
    13, // RpcDeletePrinterDriver
    14, // RpcAddPrintProcessor
    27, // RpcSetPrinterData
    30, // RpcAddForm
    31, // RpcDeleteForm
    33, // RpcSetForm
    39, // RpcDeletePort
    46, // RpcAddMonitor
    47, // RpcDeleteMonitor
    48, // RpcDeletePrintProcessor
    61, // RpcAddPortEx
    70, // RpcAddPrinterEx
    71, // RpcSetPort
    73, // RpcDeletePrinterData
    77, // RpcSetPrinterDataEx
    81, // RpcDeletePrinterDataEx
    82, // RpcDeletePrinterKey
    84, // RpcDeletePrinterDriverEx
    85, // RpcAddPerMachineConnection
    86, // RpcDeletePerMachineConnection
    88, // RpcXcvData
    89, // RpcAddPrinterDriverEx
];

/// A stub that does not decode (RPC_X_BAD_STUB_DATA).
pub(crate) const BAD_STUB_DATA: u32 = 0x0000_06F7;

/// nca_s_fault_context_mismatch: a handle the server did not open.
pub(crate) const CONTEXT_MISMATCH: u32 = 0x1C00_001A;

/// Largest output buffer a call may ask for.
pub(crate) const MAX_OUT_BUFFER: usize = 512 << 10;

pub(crate) fn bad_stub() -> Fault {
    Fault {
        status: BAD_STUB_DATA,
        did_not_execute: false,
    }
}

/// What one call answers from.
pub(crate) struct Ctx<'a> {
    pub service: &'a dyn PrintService,
    /// The server's name, as `\\NAME` shows it.
    pub server: &'a str,
    pub who: &'a Identity,
}

impl Ctx<'_> {
    pub(crate) async fn printers(&self) -> Vec<PrinterInfo> {
        self.service.printers(self.who).await
    }

    /// The printer named `share`, ignoring ASCII case, if `who` sees it.
    pub(crate) async fn printer(&self, share: &str) -> Option<PrinterInfo> {
        self.printers()
            .await
            .into_iter()
            .find(|p| p.share.eq_ignore_ascii_case(share))
    }
}

/// Runs the operation `opnum` for `caller` against the server's print
/// service and returns the response stub.
pub(crate) async fn serve(
    server: &ServerState,
    caller: &Caller,
    session: &mut Session,
    opnum: u16,
    stub: &[u8],
) -> Result<Vec<u8>, Fault> {
    let Some(service) = server.config.print_service.as_ref() else {
        return Err(Fault {
            status: NCA_OP_RNG_ERROR,
            did_not_execute: true,
        });
    };
    let ctx = Ctx {
        service: &**service,
        server: &server.config.netbios_name,
        who: &caller.identity,
    };
    call(&ctx, session, opnum, stub).await
}

/// [`serve`] with what the call answers from given.
pub(crate) async fn call(
    ctx: &Ctx<'_>,
    session: &mut Session,
    opnum: u16,
    stub: &[u8],
) -> Result<Vec<u8>, Fault> {
    use opnum::*;
    let mut r = NdrReader::new(stub);
    let r = &mut r;
    match opnum {
        ENUM_PRINTERS => printer::enum_printers(ctx, r).await,
        OPEN_PRINTER => session.open(ctx, r, false).await,
        OPEN_PRINTER_EX => session.open(ctx, r, true).await,
        CLOSE_PRINTER => session.close(r).await,
        GET_PRINTER => printer::get_printer(ctx, session, r).await,
        GET_PRINTER_DRIVER => printer::get_driver(ctx, session, r, false).await,
        GET_PRINTER_DRIVER_2 => printer::get_driver(ctx, session, r, true).await,
        GET_PRINTER_DATA => data::get(ctx, session, r, false).await,
        GET_PRINTER_DATA_EX => data::get(ctx, session, r, true).await,
        ENUM_PRINTER_DATA => data::enum_values(ctx, session, r).await,
        ENUM_PRINTER_DATA_EX => data::enum_values_ex(ctx, session, r).await,
        ENUM_PRINTER_KEY => data::enum_key(session, r),
        ENUM_FORMS => printer::enum_forms(session, r),
        GET_FORM => printer::get_form(session, r),
        START_DOC_PRINTER => job::start_doc(ctx, session, r).await,
        START_PAGE_PRINTER => job::start_page(session, r),
        WRITE_PRINTER => job::write(session, r).await,
        END_PAGE_PRINTER => job::end_page(session, r),
        END_DOC_PRINTER => job::end_doc(session, r).await,
        ABORT_PRINTER => job::abort(session, r),
        GET_JOB => job::get_job(ctx, session, r).await,
        ENUM_JOBS => job::enum_jobs(ctx, session, r).await,
        FIND_CLOSE_PRINTER_CHANGE_NOTIFICATION => {
            session.handle(r)?;
            done(r, codes::ERROR_INVALID_HANDLE)
        }
        REMOTE_FIND_FIRST_PRINTER_CHANGE_NOTIFICATION => notify_first(session, r),
        REMOTE_FIND_FIRST_PRINTER_CHANGE_NOTIFICATION_EX => notify_first_ex(session, r),
        SET_JOB | SCHEDULE_JOB | RESET_PRINTER => {
            session.handle(r)?;
            Ok(status_only(codes::ERROR_NOT_SUPPORTED))
        }
        op if ADMINISTRATION.contains(&op) => Err(Fault {
            status: codes::ERROR_ACCESS_DENIED,
            did_not_execute: true,
        }),
        _ => Err(Fault {
            status: NCA_OP_RNG_ERROR,
            did_not_execute: true,
        }),
    }
}

/// A response stub that is the status only.
pub(crate) fn status_only(status: u32) -> Vec<u8> {
    let mut w = NdrWriter::new();
    w.u32(status);
    w.finish()
}

/// [`status_only`] for a request whose every byte was read.
pub(crate) fn done(r: &NdrReader<'_>, status: u32) -> Result<Vec<u8>, Fault> {
    if !r.at_end() {
        return Err(bad_stub());
    }
    Ok(status_only(status))
}

/// RpcRemoteFindFirstPrinterChangeNotification: no notifications are
/// sent, so the client polls. pBuffer is sent back as it came.
fn notify_first(session: &Session, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    session.handle(r)?;
    r.u32().ok_or_else(bad_stub)?;
    r.u32().ok_or_else(bad_stub)?;
    r.unique_string().ok_or_else(bad_stub)?;
    r.u32().ok_or_else(bad_stub)?;
    let len = r.u32().ok_or_else(bad_stub)?;
    if len > 512 {
        return Err(bad_stub());
    }
    let buffer = if r.pointer().ok_or_else(bad_stub)? {
        Some(r.bytes(512).ok_or_else(bad_stub)?.to_vec())
    } else {
        None
    };
    let mut w = NdrWriter::new();
    w.pointer(buffer.is_some());
    if let Some(buffer) = buffer {
        w.bytes(&buffer);
    }
    w.u32(codes::ERROR_NOT_SUPPORTED);
    Ok(w.finish())
}

/// RpcRemoteFindFirstPrinterChangeNotificationEx: as
/// [`notify_first`]. The options are not read.
fn notify_first_ex(session: &Session, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    session.handle(r)?;
    r.u32().ok_or_else(bad_stub)?;
    r.u32().ok_or_else(bad_stub)?;
    r.unique_string().ok_or_else(bad_stub)?;
    r.u32().ok_or_else(bad_stub)?;
    r.pointer().ok_or_else(bad_stub)?;
    Ok(status_only(codes::ERROR_NOT_SUPPORTED))
}

/// An `[in, out, unique, size_is(cbBuf)] BYTE*` buffer and its cbBuf, as
/// the INFO calls take them.
pub(crate) struct OutBuf {
    present: bool,
    len: usize,
}

impl OutBuf {
    /// Reads the pointer, the array and cbBuf. The array's contents are
    /// ignored (MS-RPRN 2.2.2).
    pub(crate) fn read(r: &mut NdrReader<'_>) -> Result<Self, Fault> {
        let present = r.pointer().ok_or_else(bad_stub)?;
        if present {
            r.bytes(MAX_OUT_BUFFER).ok_or_else(bad_stub)?;
        }
        let len = r.u32().ok_or_else(bad_stub)? as usize;
        if len > MAX_OUT_BUFFER {
            return Err(bad_stub());
        }
        Ok(Self { present, len })
    }

    /// Room the client gave.
    pub(crate) fn room(&self) -> usize {
        if self.present { self.len } else { 0 }
    }

    /// Writes the buffer with `data` when it fits, else zeros; returns
    /// whether it fitted.
    pub(crate) fn write(&self, w: &mut NdrWriter, data: &[u8]) -> bool {
        let fits = data.len() <= self.room();
        w.pointer(self.present);
        if self.present {
            let mut buf = vec![0; self.len];
            if fits {
                buf[..data.len()].copy_from_slice(data);
            }
            w.bytes(&buf);
        }
        fits
    }
}

/// Reads a u32 or faults.
pub(crate) fn u32_of(r: &mut NdrReader<'_>) -> Result<u32, Fault> {
    r.u32().ok_or_else(bad_stub)
}

/// Reads a `[string]` (ref) argument or faults.
pub(crate) fn string_of(r: &mut NdrReader<'_>) -> Result<String, Fault> {
    r.string()
        .map(|units| String::from_utf16_lossy(&units))
        .ok_or_else(bad_stub)
}

/// Reads a `[string, unique]` argument or faults.
pub(crate) fn unique_string_of(r: &mut NdrReader<'_>) -> Result<Option<String>, Fault> {
    r.unique_string().ok_or_else(bad_stub)
}

/// The status of an INFO call: success, or ERROR_INSUFFICIENT_BUFFER.
pub(crate) fn fitted(fits: bool) -> u32 {
    if fits {
        codes::ERROR_SUCCESS
    } else {
        codes::ERROR_INSUFFICIENT_BUFFER
    }
}

/// The length as a DWORD.
pub(crate) fn dword(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// The handles [`fuzz_request`] has open: the server and printer PRT01.
#[cfg(any(test, fuzzing, feature = "fuzzing"))]
pub(crate) const FUZZ_HANDLES: [[u8; 20]; 2] = [[1; 20], [2; 20]];

/// Runs `data` as a spoolss request stub against a print service in
/// memory, with [`FUZZ_HANDLES`] open: the first byte picks the
/// operation, one of those served or, with the second byte, any opnum.
/// A call never opens more than the handle limit, and every response
/// that is not a fault ends with a status.
#[cfg(any(test, fuzzing, feature = "fuzzing"))]
pub(crate) fn fuzz_request(data: &[u8]) {
    use std::sync::OnceLock;

    const SERVED: [u16; 28] = [
        0, 1, 3, 4, 8, 11, 17, 18, 19, 20, 21, 23, 26, 29, 32, 34, 53, 56, 62, 65, 69, 72, 78, 79,
        80, 2, 25, 52,
    ];
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let Some((&pick, stub)) = data.split_first() else {
        return;
    };
    let (opnum, stub) = match SERVED.get(usize::from(pick)) {
        Some(&opnum) => (opnum, stub),
        None => match stub.split_first() {
            Some((&opnum, stub)) => (u16::from(opnum), stub),
            None => return,
        },
    };
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
    });
    let print = mem::MemPrint::default();
    let who = Identity::User {
        user: "FUZZ".into(),
        domain: String::new(),
    };
    let mut session = Session::default();
    session.insert(FUZZ_HANDLES[0], session::Target::Server);
    session.insert(FUZZ_HANDLES[1], session::Target::Printer("PRT01".into()));
    let ctx = Ctx {
        service: &print,
        server: "FUZZ",
        who: &who,
    };
    let result = runtime.block_on(call(&ctx, &mut session, opnum, stub));
    assert!(session.open_handles() <= session::MAX_HANDLES);
    if let Ok(out) = result {
        assert!(out.len() >= 4, "opnum {opnum}: a status");
    }
}
