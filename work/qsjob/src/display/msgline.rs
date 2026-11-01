// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages by identifier on the message line: their text from the
//! job's message files through the storage service, and Additional
//! Message Information when Help is pressed on one. Every display of
//! the job (panels, menus, the developer tools) words its message line
//! this way, never with text of its own.

use mi::Job;
use qir::MsgType;
use msgf::MsgDesc;
use stg::Client;
use uim::engine::LineMsg;
use wm::obj::msgq::{MsgKind, QMsg};

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The description of `id` in the message file its prefix names.
#[must_use]
pub fn describe(host: &Host, id: &str) -> Option<MsgDesc> {
    let file = sysapi::param::file_of(id);
    host.link
        .borrow_mut()
        .with(|l| l.describe(id.to_string(), file.into(), "*LIBL".into()))
}

/// A message's first-level text, its values substituted; the
/// identifier and the values when no file describes it.
#[must_use]
pub fn text_of(host: &Host, m: &LineMsg) -> String {
    if let Some(t) = &m.text {
        return t.clone();
    }
    describe(host, &m.id).map_or_else(|| m.fallback(), |d| d.format(&d.data_of(&m.data)))
}

/// The text of message `id` with `data`, for a display that draws its
/// own message line.
#[must_use]
pub fn text(host: &Host, id: &str, data: &[&str]) -> String {
    text_of(host, &LineMsg::error(id, data))
}

/// The text a service failure shows: its message when the service
/// worded one (`CPFnnnn text`), else the function check, the detail
/// kept for the job log (`panels::line`).
#[must_use]
pub fn of_error(host: &Host, e: &str) -> String {
    text_of(host, &crate::panels::line(e))
}

/// The text of message `id` with values of any kind, for a display
/// that draws its own message line.
#[must_use]
pub fn say(host: &Host, id: &str, data: &[&dyn std::fmt::Display]) -> String {
    let data: Vec<String> = data.iter().map(ToString::to_string).collect();
    let data: Vec<&str> = data.iter().map(String::as_str).collect();
    text(host, id, &data)
}

/// The message as a queue entry: what Additional Message Information
/// shows.
fn as_queued(host: &Host, m: &LineMsg) -> QMsg {
    let desc = describe(host, &m.id);
    let data = desc.as_ref().map(|d| d.data_of(&m.data)).unwrap_or_default();
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(0));
    QMsg {
        kind: if m.error { MsgKind::Diag } else { MsgKind::Info },
        severity: desc
            .as_ref()
            .map_or(0, |d| u8::try_from(d.severity).unwrap_or(99)),
        time,
        msgid: m.id.clone(),
        msgf: (sysapi::param::file_of(&m.id).into(), "QSYS".into()),
        job: (
            host.start.name.clone(),
            host.start.user.clone(),
            host.start.number,
        ),
        text: match (&m.text, &desc) {
            (Some(t), _) => t.clone(),
            (None, Some(d)) => d.format(&data),
            (None, None) => m.fallback(),
        },
        help: desc.as_ref().map(|d| d.format_help(&data)).unwrap_or_default(),
        data,
        ..QMsg::default()
    }
}

/// Additional Message Information for a message line's message; false
/// when the panel group cannot be opened.
pub fn help(surface: &mut Surface, m: &LineMsg) -> bool {
    let host = surface.host().clone();
    let q = as_queued(&host, m);
    let Ok(mut d) = crate::panels::dialog(&host, "QUIDSPMSG") else {
        return false;
    };
    crate::panels::objects::msgs::info(&host, surface, &mut d, "", &q).is_ok()
}

/// A message's data as its description's variables render it.
fn values_of(host: &Host, id: &str, data: &[u8]) -> Vec<String> {
    describe(host, id).map(|d| d.render_vars(data)).unwrap_or_default()
}

/// A command's escape as a message line's message: its identifier
/// and data, with the text the command worded it in when it did.
#[must_use]
pub fn of_fail(host: &Host, f: &Fail) -> LineMsg {
    let values = values_of(host, &f.msgid, &f.data);
    let values: Vec<&str> = values.iter().map(String::as_str).collect();
    let mut m = LineMsg::error(&f.msgid, &values);
    let last = f.text.trim_end().rsplit('\n').next().unwrap_or("").trim();
    let last = last.strip_prefix(f.msgid.as_str()).map_or(last, str::trim_start);
    if !last.is_empty() && f.data.is_empty() {
        m.text = Some(last.to_string());
    }
    m
}

/// The newest message key on the job's external queue: what a command
/// run after it sends comes after it ([`of_outcome`]).
#[must_use]
pub fn newest(job: &Job) -> u32 {
    job.ext_msgq.msgs.last().map_or(0, |m| m.key)
}

/// Run a command from a display: what it leaves on the message line
/// ([`of_outcome`]).
pub fn run(job: &mut Job, host: &Host, text: &str) -> (Outcome, Option<LineMsg>) {
    let since = newest(job);
    let out = crate::command::run(job, host, text);
    let m = of_outcome(job, host, since, &out);
    (out, m)
}

/// What a command leaves on the message line: its escape, else the
/// last completion or diagnostic it sent after key `since`; nothing
/// when it sent none (its output is never the message).
pub fn of_outcome(job: &Job, host: &Host, since: u32, out: &Outcome) -> Option<LineMsg> {
    match &out.escape {
        // An analysis error shows its first diagnostic, as IBM's line
        // shows `CPD0030` for a command that is not found.
        Some(f) if f.msgid == "CPF0001" && out.diag.is_some() => out.diag.as_ref().map(|d| of_fail(host, d)),
        Some(f) => Some(of_fail(host, f)),
        None => job
            .ext_msgq
            .msgs
            .iter()
            .rev()
            .take_while(|m| m.key > since)
            .find(|m| matches!(m.msg_type, MsgType::Comp | MsgType::Diag | MsgType::Info))
            .map(|m| {
                let values = values_of(host, &m.msgid, &m.data);
                let values: Vec<&str> = values.iter().map(String::as_str).collect();
                let mut l = LineMsg::info(&m.msgid, &values);
                l.error = m.msg_type == MsgType::Diag;
                l.text = Some(m.text.trim().to_string());
                l
            }),
    }
}

/// Give a waiting message its text, for a display that draws the text
/// itself (the developer tools).
pub fn word(host: &Host, m: &mut Option<LineMsg>) {
    if let Some(m) = m
        && m.text.is_none()
    {
        m.text = Some(text_of(host, m));
    }
}
