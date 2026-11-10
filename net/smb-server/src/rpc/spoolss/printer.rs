//! RpcEnumPrinters, RpcGetPrinter, RpcGetPrinterDriver(2), RpcEnumForms
//! and RpcGetForm (MS-RPRN 3.1.4.2.1, 3.1.4.2.6, 3.1.4.4.3, 3.1.4.4.2,
//! 3.1.4.5.4, 3.1.4.5.2).

use super::info::{self, Named};
use super::marshal::pack;
use super::session::{Session, Target};
use super::{
    Ctx, OutBuf, bad_stub, codes, dword, fitted, forms, string_of, u32_of, unique_string_of,
};
use crate::rpc::association::Fault;
use crate::rpc::ndr::{NdrReader, NdrWriter};

// RpcEnumPrinters flags (MS-RPRN 2.2.3.7).
const PRINTER_ENUM_LOCAL: u32 = 0x0000_0002;
const PRINTER_ENUM_NAME: u32 = 0x0000_0008;
const PRINTER_ENUM_SHARED: u32 = 0x0000_0020;

/// The client environments a driver is answered for (MS-RPRN 2.2.4.4).
const ENVIRONMENTS: [&str; 4] = [
    "Windows x64",
    "Windows NT x86",
    "Windows ARM64",
    "Windows IA64",
];

/// The levels RpcEnumPrinters serves.
const ENUM_LEVELS: [u32; 4] = [1, 2, 4, 5];

/// Writes an INFO call's answer: the buffer, pcbNeeded, then pcReturned
/// when `count` is given.
fn answer(out: &OutBuf, data: &[u8], count: Option<usize>, status: Option<u32>) -> Vec<u8> {
    let mut w = NdrWriter::new();
    let fits = out.write(&mut w, data);
    w.u32(dword(data.len()));
    if let Some(count) = count {
        w.u32(if fits { dword(count) } else { 0 });
    }
    w.u32(status.unwrap_or(fitted(fits)));
    w.finish()
}

/// An INFO call refused with `status`: nothing written, nothing needed.
fn refuse(out: &OutBuf, count: bool, status: u32) -> Vec<u8> {
    let mut w = NdrWriter::new();
    out.write(&mut w, &[]);
    w.u32(0);
    if count {
        w.u32(0);
    }
    w.u32(status);
    w.finish()
}

/// RpcEnumPrinters: the server's printers for a name of none, `\\NAME`,
/// with PRINTER_ENUM_LOCAL, _NAME or _SHARED; none for the other flags
/// (connections, network, remote). A printer's name is ERROR_INVALID_NAME.
pub(crate) async fn enum_printers(ctx: &Ctx<'_>, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    let flags = u32_of(r)?;
    let name = unique_string_of(r)?;
    let level = u32_of(r)?;
    let out = OutBuf::read(r)?;
    if !r.at_end() {
        return Err(bad_stub());
    }
    if !ENUM_LEVELS.contains(&level) {
        return Ok(refuse(&out, true, codes::ERROR_INVALID_LEVEL));
    }
    let names_server = match name.as_deref().unwrap_or("") {
        "" => true,
        name => name
            .strip_prefix("\\\\")
            .is_some_and(|rest| !rest.trim_end_matches('\\').contains('\\')),
    };
    if !names_server {
        return Ok(refuse(&out, true, codes::ERROR_INVALID_NAME));
    }
    let printers = if flags & (PRINTER_ENUM_LOCAL | PRINTER_ENUM_NAME | PRINTER_ENUM_SHARED) != 0 {
        ctx.printers().await
    } else {
        Vec::new()
    };
    let blocks = printers
        .iter()
        .map(|p| {
            info::printer(
                &Named {
                    server: ctx.server,
                    printer: p,
                },
                level,
            )
            .expect("a served level")
        })
        .collect();
    Ok(answer(&out, &pack(blocks), Some(printers.len()), None))
}

/// The share a printer handle names, or the status that refuses it: a
/// server handle is ERROR_INVALID_HANDLE.
fn printer_of(handle: &super::session::Handle) -> Result<&str, u32> {
    match &handle.target {
        Target::Printer(share) => Ok(share),
        Target::Server => Err(codes::ERROR_INVALID_HANDLE),
    }
}

/// RpcGetPrinter at levels 0 to 5, 7 and 8.
pub(crate) async fn get_printer(
    ctx: &Ctx<'_>,
    session: &Session,
    r: &mut NdrReader<'_>,
) -> Result<Vec<u8>, Fault> {
    let handle = session.handle(r)?;
    let level = u32_of(r)?;
    let out = OutBuf::read(r)?;
    if !r.at_end() {
        return Err(bad_stub());
    }
    let share = match printer_of(handle) {
        Ok(share) => share,
        Err(status) => return Ok(refuse(&out, false, status)),
    };
    let Some(printer) = ctx.printer(share).await else {
        return Ok(refuse(&out, false, codes::ERROR_PRINTER_DELETED));
    };
    let named = Named {
        server: ctx.server,
        printer: &printer,
    };
    match info::printer(&named, level) {
        Some(block) => Ok(answer(&out, &pack(vec![block]), None, None)),
        None => Ok(refuse(&out, false, codes::ERROR_INVALID_LEVEL)),
    }
}

/// RpcGetPrinterDriver and RpcGetPrinterDriver2 (`v2`, with the client's
/// version in and the server's out) at levels 1, 2, 3, 4, 6 and 8. With
/// no environment the client means the server's own, x64.
pub(crate) async fn get_driver(
    ctx: &Ctx<'_>,
    session: &Session,
    r: &mut NdrReader<'_>,
    v2: bool,
) -> Result<Vec<u8>, Fault> {
    let handle = session.handle(r)?;
    let environment = unique_string_of(r)?.unwrap_or_else(|| ENVIRONMENTS[0].to_string());
    let level = u32_of(r)?;
    let out = OutBuf::read(r)?;
    if v2 {
        u32_of(r)?;
        u32_of(r)?;
    }
    if !r.at_end() {
        return Err(bad_stub());
    }
    let result = match printer_of(handle) {
        Err(status) => Err(status),
        Ok(share) => match ctx.printer(share).await {
            None => Err(codes::ERROR_PRINTER_DELETED),
            Some(printer) => driver_block(ctx.server, &printer, &environment, level),
        },
    };
    let mut w = NdrWriter::new();
    let (status, version) = match result {
        Ok((data, version)) => {
            let fits = out.write(&mut w, &data);
            w.u32(dword(data.len()));
            (fitted(fits), version)
        }
        Err(status) => {
            out.write(&mut w, &[]);
            w.u32(0);
            (status, 0)
        }
    };
    if v2 {
        w.u32(version);
        w.u32(0);
    }
    w.u32(status);
    Ok(w.finish())
}

/// The packed DRIVER_INFO of `printer` and its version.
fn driver_block(
    server: &str,
    printer: &crate::print::PrinterInfo,
    environment: &str,
    level: u32,
) -> Result<(Vec<u8>, u32), u32> {
    let Some(env) = ENVIRONMENTS
        .iter()
        .find(|e| e.eq_ignore_ascii_case(environment))
    else {
        return Err(codes::ERROR_INVALID_ENVIRONMENT);
    };
    let files = if printer.driver_files.is_empty() {
        None
    } else {
        Some(
            printer
                .driver_files
                .iter()
                .find(|f| f.environment.eq_ignore_ascii_case(env))
                .ok_or(codes::ERROR_UNKNOWN_PRINTER_DRIVER)?,
        )
    };
    let version = files.map_or(info::DRIVER_VERSION_3, |f| f.version);
    let block = info::driver(level, &printer.driver, env, server, files)
        .ok_or(codes::ERROR_INVALID_LEVEL)?;
    Ok((pack(vec![block]), version))
}

/// RpcEnumForms at levels 1 and 2, on any handle.
pub(crate) fn enum_forms(session: &Session, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    session.handle(r)?;
    let level = u32_of(r)?;
    let out = OutBuf::read(r)?;
    if !r.at_end() {
        return Err(bad_stub());
    }
    let blocks: Option<Vec<_>> = forms::FORMS.iter().map(|f| forms::info(f, level)).collect();
    match blocks {
        Some(blocks) => Ok(answer(&out, &pack(blocks), Some(forms::FORMS.len()), None)),
        None => Ok(refuse(&out, true, codes::ERROR_INVALID_LEVEL)),
    }
}

/// RpcGetForm at levels 1 and 2.
pub(crate) fn get_form(session: &Session, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    session.handle(r)?;
    let name = string_of(r)?;
    let level = u32_of(r)?;
    let out = OutBuf::read(r)?;
    if !r.at_end() {
        return Err(bad_stub());
    }
    let Some(form) = forms::find(&name) else {
        return Ok(refuse(&out, false, codes::ERROR_INVALID_FORM_NAME));
    };
    match forms::info(form, level) {
        Some(block) => Ok(answer(&out, &pack(vec![block]), None, None)),
        None => Ok(refuse(&out, false, codes::ERROR_INVALID_LEVEL)),
    }
}
