//! Printing through a handle (MS-RPRN 3.1.4.9: RpcStartDocPrinter,
//! RpcStartPagePrinter, RpcWritePrinter, RpcEndPagePrinter,
//! RpcEndDocPrinter, RpcAbortPrinter) and the user's own jobs
//! (3.1.4.3: RpcGetJob, RpcEnumJobs).

use super::info::{self, Named};
use super::marshal::pack;
use super::session::{Doc, PRINTER_ACCESS_USE, Session, Target};
use super::{Ctx, OutBuf, bad_stub, codes, done, dword, fitted, status_only, u32_of};
use crate::print::DocInfo;
use crate::rpc::association::Fault;
use crate::rpc::ndr::{NdrReader, NdrWriter};

/// Longest document name taken, in UTF-16 units.
const MAX_DOC_NAME: u32 = 32_767;

/// Largest RpcWritePrinter buffer.
const MAX_WRITE: usize = 1 << 20;

/// RpcStartDocPrinter: DOC_INFO_CONTAINER at level 1, RAW data only, no
/// output file. Answers the job id.
pub(crate) async fn start_doc(
    ctx: &Ctx<'_>,
    session: &mut Session,
    r: &mut NdrReader<'_>,
) -> Result<Vec<u8>, Fault> {
    let handle = session.handle_mut(r)?;
    let level = u32_of(r)?;
    if u32_of(r)? != level {
        return Err(bad_stub());
    }
    let present = r.pointer().ok_or_else(bad_stub)?;
    let doc = if present && level == 1 {
        let name = r.pointer().ok_or_else(bad_stub)?;
        let output = r.pointer().ok_or_else(bad_stub)?;
        let datatype = r.pointer().ok_or_else(bad_stub)?;
        let mut text = |present: bool| -> Result<Option<String>, Fault> {
            if !present {
                return Ok(None);
            }
            r.string_up_to(MAX_DOC_NAME)
                .map(|units| Some(String::from_utf16_lossy(&units)))
                .ok_or_else(bad_stub)
        };
        Some((text(name)?, text(output)?, text(datatype)?))
    } else {
        None
    };
    let reply = |status: u32, job: u32| {
        let mut w = NdrWriter::new();
        w.u32(job);
        w.u32(status);
        Ok(w.finish())
    };
    let Some((name, output, datatype)) = doc else {
        let status = if level == 1 {
            codes::ERROR_INVALID_PARAMETER
        } else {
            codes::ERROR_INVALID_LEVEL
        };
        return reply(status, 0);
    };
    let Target::Printer(share) = &handle.target else {
        return reply(codes::ERROR_INVALID_HANDLE, 0);
    };
    if handle.granted & PRINTER_ACCESS_USE == 0 {
        return reply(codes::ERROR_ACCESS_DENIED, 0);
    }
    if handle.doc.is_some() {
        return reply(codes::ERROR_INVALID_PRINTER_STATE, 0);
    }
    if output.is_some_and(|o| !o.is_empty()) {
        return reply(codes::ERROR_INVALID_PARAMETER, 0);
    }
    if datatype.is_some_and(|d| !d.eq_ignore_ascii_case(info::RAW)) {
        return reply(codes::ERROR_INVALID_DATATYPE, 0);
    }
    let doc = DocInfo {
        document: name.unwrap_or_default(),
        machine: handle.machine.clone(),
    };
    match ctx.service.start_job(ctx.who, share, &doc).await {
        Ok(job) => {
            let id = job.id();
            handle.doc = Some(Doc {
                job,
                pages: 0,
                in_page: false,
            });
            reply(codes::ERROR_SUCCESS, id)
        }
        Err(e) => reply(e.win32(), 0),
    }
}

/// The open document of a handle, or ERROR_SPL_NO_STARTDOC.
fn doc_of<'a>(
    session: &'a mut Session,
    r: &mut NdrReader<'_>,
) -> Result<Result<&'a mut Doc, u32>, Fault> {
    let handle = session.handle_mut(r)?;
    Ok(handle.doc.as_mut().ok_or(codes::ERROR_SPL_NO_STARTDOC))
}

/// RpcStartPagePrinter.
pub(crate) fn start_page(session: &mut Session, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    let status = match doc_of(session, r)? {
        Ok(doc) => {
            doc.in_page = true;
            codes::ERROR_SUCCESS
        }
        Err(status) => status,
    };
    done(r, status)
}

/// RpcEndPagePrinter: one more page.
pub(crate) fn end_page(session: &mut Session, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    let status = match doc_of(session, r)? {
        Ok(doc) => {
            doc.in_page = false;
            doc.pages += 1;
            codes::ERROR_SUCCESS
        }
        Err(status) => status,
    };
    done(r, status)
}

/// RpcWritePrinter: the bytes go to the job; pcWritten is all of them or
/// none.
pub(crate) async fn write(session: &mut Session, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    let doc = doc_of(session, r)?;
    let data = r.bytes(MAX_WRITE).ok_or_else(bad_stub)?;
    if u32_of(r)? as usize != data.len() || !r.at_end() {
        return Err(bad_stub());
    }
    let (written, status) = match doc {
        Err(status) => (0, status),
        Ok(doc) => match doc.job.write(data).await {
            Ok(()) => (dword(data.len()), codes::ERROR_SUCCESS),
            Err(e) => (0, e.win32()),
        },
    };
    let mut w = NdrWriter::new();
    w.u32(written);
    w.u32(status);
    Ok(w.finish())
}

/// RpcEndDocPrinter: the job prints.
pub(crate) async fn end_doc(
    session: &mut Session,
    r: &mut NdrReader<'_>,
) -> Result<Vec<u8>, Fault> {
    let handle = session.handle_mut(r)?;
    if !r.at_end() {
        return Err(bad_stub());
    }
    let status = match handle.doc.take() {
        None => codes::ERROR_SPL_NO_STARTDOC,
        Some(doc) => match doc.job.end(doc.pages).await {
            Ok(()) => codes::ERROR_SUCCESS,
            Err(e) => e.win32(),
        },
    };
    Ok(status_only(status))
}

/// RpcAbortPrinter: the job is abandoned.
pub(crate) fn abort(session: &mut Session, r: &mut NdrReader<'_>) -> Result<Vec<u8>, Fault> {
    let handle = session.handle_mut(r)?;
    let status = match handle.doc.take() {
        None => codes::ERROR_SPL_NO_STARTDOC,
        Some(_) => codes::ERROR_SUCCESS,
    };
    done(r, status)
}

/// RpcGetJob at levels 1 and 2: a job of the caller's on the printer.
pub(crate) async fn get_job(
    ctx: &Ctx<'_>,
    session: &Session,
    r: &mut NdrReader<'_>,
) -> Result<Vec<u8>, Fault> {
    let handle = session.handle(r)?;
    let id = u32_of(r)?;
    let level = u32_of(r)?;
    let out = OutBuf::read(r)?;
    if !r.at_end() {
        return Err(bad_stub());
    }
    let mut w = NdrWriter::new();
    let result = jobs_block(ctx, &handle.target, level, |jobs| {
        jobs.iter()
            .position(|j| j.id == id)
            .map(|i| i..i + 1)
            .ok_or(codes::ERROR_INVALID_PARAMETER)
    })
    .await;
    let status = match result {
        Ok((data, _)) => {
            let fits = out.write(&mut w, &data);
            w.u32(dword(data.len()));
            fitted(fits)
        }
        Err(status) => {
            out.write(&mut w, &[]);
            w.u32(0);
            status
        }
    };
    w.u32(status);
    Ok(w.finish())
}

/// RpcEnumJobs at levels 1 and 2: `NoJobs` of the caller's jobs from
/// position `FirstJob`.
pub(crate) async fn enum_jobs(
    ctx: &Ctx<'_>,
    session: &Session,
    r: &mut NdrReader<'_>,
) -> Result<Vec<u8>, Fault> {
    let handle = session.handle(r)?;
    let first = u32_of(r)? as usize;
    let count = u32_of(r)? as usize;
    let level = u32_of(r)?;
    let out = OutBuf::read(r)?;
    if !r.at_end() {
        return Err(bad_stub());
    }
    let result = jobs_block(ctx, &handle.target, level, |jobs| {
        let start = first.min(jobs.len());
        Ok(start..start.saturating_add(count).min(jobs.len()))
    })
    .await;
    let mut w = NdrWriter::new();
    let status = match result {
        Ok((data, n)) => {
            let fits = out.write(&mut w, &data);
            w.u32(dword(data.len()));
            w.u32(if fits { dword(n) } else { 0 });
            fitted(fits)
        }
        Err(status) => {
            out.write(&mut w, &[]);
            w.u32(0);
            w.u32(0);
            status
        }
    };
    w.u32(status);
    Ok(w.finish())
}

/// The packed JOB_INFO of the jobs `pick` chooses, and how many.
async fn jobs_block(
    ctx: &Ctx<'_>,
    target: &Target,
    level: u32,
    pick: impl FnOnce(&[crate::print::JobInfo]) -> Result<std::ops::Range<usize>, u32>,
) -> Result<(Vec<u8>, usize), u32> {
    let Target::Printer(share) = target else {
        return Err(codes::ERROR_INVALID_HANDLE);
    };
    if !matches!(level, 1 | 2) {
        return Err(codes::ERROR_INVALID_LEVEL);
    }
    let printer = ctx
        .printer(share)
        .await
        .ok_or(codes::ERROR_PRINTER_DELETED)?;
    let jobs = ctx.service.jobs(ctx.who, share).await;
    let range = pick(&jobs)?;
    let named = Named {
        server: ctx.server,
        printer: &printer,
    };
    let n = range.len();
    let blocks = range
        .map(|i| {
            let position = u32::try_from(i + 1).unwrap_or(u32::MAX);
            info::job(&named, &jobs[i], position, level).expect("a served level")
        })
        .collect();
    Ok((pack(blocks), n))
}
