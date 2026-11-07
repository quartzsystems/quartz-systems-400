// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Spool` service (docs/QAP.md, version 3) over the spooled-file
//! commands' own functions (`splcmd.rs`, `spool.rs`): listed as
//! `WRKSPLF` and `WRKOUTQ` list, read as `DSPSPLF` reads, held,
//! released, deleted and moved as `HLDSPLF`, `RLSSPLF`, `DLTSPLF` and
//! `CHGSPLFA OUTQ` do, with their authority checks and `SF` entries, as
//! the job's signed-on user. Pages and formats come from
//! `qapd::spool::read`.

use qap_proto::ops::spool::{
    DeviceType, ListOutputQueues, ListSpooledFiles, OutputQueue, OutputQueues, ReadSpooledFile,
    SpoolStatus, SpooledData, SpooledFile, SpooledFileId, SpooledFiles,
};
use qap_proto::types::{JobId, ObjectName};
use qapd::spool::{self as read, Unreadable};
use stg::QapRefused;
use wm::obj::params::qualified;
use wm::obj::splf::{SplfAttrs, Status};

use super::super::served::JobCtx;
use super::super::{splcmd, spool};
use super::{ERROR, NOT_AUTHORIZED, connection_job, refused};
use crate::ipl::system::System;

const NOT_FOUND: u16 = 7;
const INVALID_REQUEST: u16 = 8;
const IN_USE: u16 = 14;

/// A command's error (`CPFnnnn text`) as the client is told it.
fn failed(e: &str) -> QapRefused {
    let id = e
        .split_whitespace()
        .next()
        .filter(|w| w.len() == 7 && w.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("CPF9898");
    let text = e.strip_prefix(id).unwrap_or(e).trim();
    let status = match id {
        "CPF3309" | "CPF3357" | "CPF2110" | "CPF2204" | "CPF9801" | "CPF2105" => NOT_FOUND,
        "CPF3303" | "CPF9802" | "CPF2189" | "CPF2182" | "CPF22E9" => NOT_AUTHORIZED,
        "CPF3330" => IN_USE,
        _ => ERROR,
    };
    refused(status, id, text, &[])
}

/// The signed-on user the job runs as.
fn me(ctx: &JobCtx) -> String {
    ctx.session.borrow().principal.user.clone()
}

/// A spooled file's status as QAP names it. `WTR` is printing; QS/400
/// has no state for a file being written but `OPN`.
fn status_of(s: Status) -> SpoolStatus {
    match s {
        Status::Rdy => SpoolStatus::Ready,
        Status::Hld => SpoolStatus::Held,
        Status::Opn => SpoolStatus::Open,
        Status::Clo => SpoolStatus::Closed,
        Status::Wtr => SpoolStatus::Printing,
        Status::Msgw => SpoolStatus::Message,
        Status::Pnd | Status::Dfr => SpoolStatus::Pending,
        Status::Sav => SpoolStatus::Saved,
    }
}

/// `DEVTYPE` as QAP names it.
fn device_of(devtype: &str) -> DeviceType {
    match devtype.trim().to_ascii_uppercase().as_str() {
        "*AFPDS" | "*AFPDSLINE" | "*IPDS" => DeviceType::Afp,
        "*USERASCII" => DeviceType::Ascii,
        "*LINE" => DeviceType::Line,
        _ => DeviceType::Scs,
    }
}

fn object(q: &str) -> ObjectName {
    let (library, name) = qualified(q);
    ObjectName { library, name }
}

fn file_of(a: &SplfAttrs) -> SpooledFile {
    SpooledFile {
        id: SpooledFileId {
            job: JobId {
                name: a.job.0.clone(),
                user: a.job.1.clone(),
                number: format!("{:06}", a.job.2),
            },
            file: a.name.clone(),
            number: a.number,
        },
        status: status_of(a.status),
        outq: object(&a.outq),
        user: a.job.1.clone(),
        user_data: a.usrdta.clone(),
        form_type: if a.formtype.trim().is_empty() {
            "*STD".into()
        } else {
            a.formtype.clone()
        },
        device_type: device_of(&a.devtype),
        pages: a.pages,
        copies: a.copies,
        size: a.data_len,
        created: i64::try_from(a.created / 1000).unwrap_or(i64::MAX),
    }
}

/// The spooled file `id` names.
fn find(system: &System, id: &SpooledFileId) -> Result<u32, QapRefused> {
    let job = format!(
        "{}/{}/{}",
        id.job.number.trim(),
        id.job.user.trim().to_ascii_uppercase(),
        id.job.name.trim().to_ascii_uppercase()
    );
    spool::find(
        system,
        &id.file.trim().to_ascii_uppercase(),
        &job,
        &id.number.to_string(),
    )
    .map_err(|e| failed(&e))
}

/// A page of `v` from `start`, at most `max` (0 for all).
fn page<T: Clone>(v: &[T], start: u32, max: u32) -> Vec<T> {
    let start = usize::try_from(start).unwrap_or(usize::MAX);
    let max = if max == 0 {
        usize::MAX
    } else {
        usize::try_from(max).unwrap_or(usize::MAX)
    };
    v.iter().skip(start).take(max).cloned().collect()
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `qap_splf_list`: `WRKSPLF SELECT(user)`'s files, or `WRKOUTQ`'s for
/// one queue, then the filters.
pub fn list(
    system: &mut System,
    ctx: &JobCtx,
    req: &ListSpooledFiles,
) -> Result<SpooledFiles, QapRefused> {
    connection_job(ctx)?;
    let user = match req.user.trim().to_ascii_uppercase().as_str() {
        "" | "*CURRENT" => Some(me(ctx)),
        "*ALL" => None,
        u => {
            if super::super::hostsvr::read(system, u).is_none() {
                return Err(refused(NOT_FOUND, "CPF2204", "", &[u]));
            }
            Some(u.to_string())
        }
    };
    let files: Vec<SplfAttrs> = match &req.outq {
        Some(q) => {
            let q = format!(
                "{}/{}",
                q.library.trim().to_ascii_uppercase(),
                q.name.trim().to_ascii_uppercase()
            );
            spool::on_queue(system, &q).map_err(|e| failed(&e))?.2
        }
        None => splcmd::files_of(system, None, None)
            .into_iter()
            .cloned()
            .collect(),
    };
    let hits: Vec<SpooledFile> = files
        .iter()
        .filter(|a| user.as_deref().is_none_or(|u| a.job.1 == u))
        .map(file_of)
        .filter(|f| req.status.is_none_or(|s| f.status == s))
        .filter(|f| {
            req.form_type
                .as_deref()
                .is_none_or(|t| f.form_type.eq_ignore_ascii_case(t.trim()))
        })
        .filter(|f| {
            req.user_data
                .as_deref()
                .is_none_or(|d| f.user_data.trim() == d.trim())
        })
        .collect();
    Ok(SpooledFiles {
        total: count(hits.len()),
        files: page(&hits, req.start, req.max),
    })
}

/// `qap_splf_read`: checked and audited as `DSPSPLF`, then the pages.
pub fn read(
    system: &mut System,
    ctx: &JobCtx,
    req: &ReadSpooledFile,
) -> Result<SpooledData, QapRefused> {
    connection_job(ctx)?;
    let id = find(system, &req.id)?;
    let (a, data) = splcmd::display(system, id).map_err(|e| failed(&e))?;
    let f = file_of(&a);
    let number = a.number.to_string();
    read::read(f.status, f.device_type, &data, req).map_err(|e| match e {
        Unreadable::InUse => refused(IN_USE, read::IN_USE, "", &[&a.name, &number]),
        Unreadable::Format => refused(
            ERROR,
            read::NO_FORMAT,
            "",
            &[
                &a.name,
                &number,
                read::format_name(req.format),
                read::device_name(f.device_type),
            ],
        ),
        Unreadable::Range => refused(INVALID_REQUEST, "CPD0084", "", &["FIRST_PAGE"]),
    })
}

/// `qap_splf_hold`, as `HLDSPLF`.
pub fn hold(
    system: &mut System,
    ctx: &JobCtx,
    id: &SpooledFileId,
    immediately: bool,
) -> Result<(), QapRefused> {
    connection_job(ctx)?;
    let id = find(system, id)?;
    splcmd::may_act(system, id, "hldsplf").map_err(|e| failed(&e))?;
    splcmd::hold(system, id, immediately).map_err(|e| failed(&e))
}

/// `qap_splf_release`, as `RLSSPLF`.
pub fn release(system: &mut System, ctx: &JobCtx, id: &SpooledFileId) -> Result<(), QapRefused> {
    connection_job(ctx)?;
    let id = find(system, id)?;
    splcmd::may_act(system, id, "rlssplf").map_err(|e| failed(&e))?;
    splcmd::release(system, id)
        .map(|_| ())
        .map_err(|e| failed(&e))
}

/// `qap_splf_delete`, as `DLTSPLF`.
pub fn delete(system: &mut System, ctx: &JobCtx, id: &SpooledFileId) -> Result<(), QapRefused> {
    connection_job(ctx)?;
    let id = find(system, id)?;
    splcmd::may_act(system, id, "dltsplf").map_err(|e| failed(&e))?;
    splcmd::delete(system, id).map_err(|e| failed(&e))
}

/// `qap_splf_move`, as `CHGSPLFA OUTQ`.
pub fn move_to(
    system: &mut System,
    ctx: &JobCtx,
    id: &SpooledFileId,
    outq: &ObjectName,
    first: bool,
) -> Result<(), QapRefused> {
    connection_job(ctx)?;
    let id = find(system, id)?;
    splcmd::may_act(system, id, "chgsplfa").map_err(|e| failed(&e))?;
    let lib = outq.library.trim().to_ascii_uppercase();
    let q = format!(
        "{}/{}",
        if lib.is_empty() { "*LIBL" } else { &lib },
        outq.name.trim().to_ascii_uppercase()
    );
    splcmd::move_file(system, id, &q, first)
        .map(|_| ())
        .map_err(|e| failed(&e))
}

/// `qap_outq_list`: `WRKOUTQ`'s queues in every library, the job's
/// library list, or one library.
pub fn queues(
    system: &mut System,
    ctx: &JobCtx,
    req: &ListOutputQueues,
) -> Result<OutputQueues, QapRefused> {
    connection_job(ctx)?;
    let lib = req.library.trim().to_ascii_uppercase();
    let libs: Option<Vec<String>> = match lib.as_str() {
        "" | "*ALL" => None,
        "*LIBL" => Some(
            system
                .attrs
                .get(&ctx.job.number)
                .map(|a| a.libl.search())
                .unwrap_or_default(),
        ),
        l => Some(vec![l.to_string()]),
    };
    let found = splcmd::queues(system, libs.as_deref(), lib != "*LIBL").map_err(|e| failed(&e))?;
    let hits: Vec<OutputQueue> = found
        .into_iter()
        .map(|q| OutputQueue {
            outq: ObjectName {
                library: q.lib,
                name: q.name,
            },
            text: q.text,
            files: count(q.oq.files.len()),
            writer: q.oq.writer.clone(),
            held: q.oq.held,
        })
        .collect();
    Ok(OutputQueues {
        total: count(hits.len()),
        queues: page(&hits, req.start, req.max),
    })
}
