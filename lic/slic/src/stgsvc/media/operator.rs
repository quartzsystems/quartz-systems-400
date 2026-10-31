// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the system operator hears about media: inquiries (mount the
//! next volume, an active file in the way) answered with their default
//! reply when nobody answers at once, attention messages (wrong volume,
//! write protected, end of volume), and for device and media errors a
//! product activity log entry and a problem in the problem log.

use prb::PrbType;
use wm::obj::msgq::{MsgKind, QMsg, Status};

use crate::ipl::system::{System, now_unix};

/// Load the next volume on a tape device (an inquiry, replies C G).
pub(crate) const NEXT_VOLUME: &str = "CPA4075";
/// An active file is in the way (an inquiry, replies C I).
pub(crate) const ACTIVE_FILE: &str = "CPA4036";

fn send(system: &mut System, m: QMsg) {
    let (id, sev, text) = (m.msgid.clone(), m.severity, m.text.clone());
    if let Err(e) = crate::stgsvc::msgs::send(system, "QSYS", "QSYSOPR", m) {
        eprintln!("qsstg: QSYSOPR: {e}");
    }
    crate::stgsvc::prblog::message_sent(system, &id, sev, &text);
}

fn message(system: &System, msgid: &str, data: &[(&str, usize)]) -> QMsg {
    let data = cmd::error::data(data);
    let number = system.current_job;
    QMsg {
        msgid: msgid.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        severity: 0,
        text: crate::stgsvc::hist::message_text(msgid, &data),
        data,
        job: crate::stgsvc::msgs::sender(system, number),
        from_pgm: OPERATOR_PGM.into(),
        ..QMsg::default()
    }
}

/// How an inquiry was answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The reply, upper case.
    Reply(String),
    /// Left on `QSYSOPR` for the operator; the operation ends and the
    /// reply is taken when it runs again.
    Waiting,
}

impl Answer {
    /// Whether the reply is `r`.
    pub(crate) fn is(&self, r: &str) -> bool {
        matches!(self, Self::Reply(x) if x.eq_ignore_ascii_case(r))
    }

    /// The reply as the commands' texts show it.
    pub(crate) fn shown(&self) -> String {
        match self {
            Self::Reply(r) => format!("the reply was {r}"),
            Self::Waiting => "the inquiry waits for its reply on QSYSOPR".into(),
        }
    }
}

/// The current job's `INQMSGRPY`.
fn inqmsgrpy(system: &System) -> String {
    system
        .attrs
        .get(&system.current_job)
        .map(|a| a.inqmsgrpy.clone())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "*RQD".into())
}

/// The reply list's reply to `msgid` with `data`, unless it says
/// `*RQD`; `*DFT` is `dft`.
fn from_reply_list(system: &mut System, msgid: &str, data: &[u8], dft: &str) -> Option<String> {
    let s = system.storage.as_mut()?;
    let qsys = s.qsys?;
    let mut txn = s.store.begin().ok()?;
    let list = wm::obj::rpyl::load(&mut txn, qsys).ok();
    txn.abort();
    let text = ccsid::from_ebcdic(data);
    let r = list?.find(msgid, &text)?.reply.clone();
    match r.as_str() {
        "*RQD" => None,
        "*DFT" => Some(dft.to_string()),
        _ => Some(r),
    }
}

/// An inquiry like this one already on `QSYSOPR`: answered by the
/// operator with a reply not taken yet (the reply is taken: marked
/// old), or still waiting.
fn earlier(system: &mut System, msgid: &str, data: &[u8]) -> Option<Answer> {
    crate::stgsvc::msgs::with_queue(system, "msgq.receive", "QSYS", "QSYSOPR", |q, _| {
        let same = |m: &QMsg| m.kind == MsgKind::Inq && m.msgid == msgid && m.data == data && m.from_pgm == OPERATOR_PGM;
        let mut found = None;
        let mut take = None;
        for m in q.msgs.iter().rev().filter(|m| same(m)) {
            if m.status == Status::Unanswered {
                found = Some(Answer::Waiting);
                break;
            }
            let fresh = q.msgs.iter().any(|r| r.key == m.reply_key && r.kind == MsgKind::Rpy && r.status == Status::New);
            if fresh {
                found = Some(Answer::Reply(m.reply.trim().to_ascii_uppercase()));
                take = Some(m.reply_key);
                break;
            }
        }
        if let Some(k) = take {
            let _ = q.receive(None, Some(k), false);
        }
        Ok((found, take.is_some()))
    })
    .ok()
    .flatten()
}

/// The program the media inquiries come from.
const OPERATOR_PGM: &str = "QTAERR";

/// Ask the operator. A reply the operator already sent to the same
/// inquiry (`SNDRPYMSG`) is taken first; otherwise the job's
/// `INQMSGRPY` decides: `*DFT` takes the default, `*SYSRPYL` the reply
/// list's entry, and `*RQD` (or no entry) leaves the inquiry waiting.
/// An inquiry answered here shows its reply on `QSYSOPR`.
pub(crate) fn inquiry(system: &mut System, msgid: &str, data: &[(&str, usize)], dft: &str) -> Answer {
    let m = message(system, msgid, data);
    if let Some(a) = earlier(system, msgid, &m.data) {
        return a;
    }
    let reply = match inqmsgrpy(system).as_str() {
        "*DFT" => Some(dft.to_string()),
        "*SYSRPYL" => from_reply_list(system, msgid, &m.data, dft),
        _ => None,
    };
    let mut m = m;
    m.kind = MsgKind::Inq;
    m.severity = 99;
    m.dft = dft.into();
    m.reply_to = ("QSYSOPR".into(), "QSYS".into());
    let key = match crate::stgsvc::msgs::send(system, "QSYS", "QSYSOPR", m) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("qsstg: QSYSOPR: {e}");
            return Answer::Reply(dft.to_string());
        }
    };
    let Some(r) = reply else { return Answer::Waiting };
    let from = crate::stgsvc::msgs::sender(system, system.current_job);
    let r = r.trim().to_ascii_uppercase();
    let rr = r.clone();
    let _ = crate::stgsvc::msgs::with_queue(system, "msgq.reply", "QSYS", "QSYSOPR", |q, _| {
        let rk = q.reply(key, &rr, from, crate::stgsvc::msgs::now_micros()).map_err(|e| e.to_string())?;
        let _ = q.receive(None, Some(rk), false);
        Ok(((), true))
    });
    Answer::Reply(r)
}

/// Tell the operator.
pub(crate) fn attention(system: &mut System, msgid: &str, data: &[(&str, usize)]) {
    let mut m = message(system, msgid, data);
    m.severity = 40;
    send(system, m);
}

/// A device or media error on resource `rsrc`: an attention to the
/// operator, a product activity log entry, and a problem.
pub(crate) fn device_error(system: &mut System, rsrc: &str, dev: &str, msgid: &str, media: bool) {
    attention(system, msgid, &[(dev, 10)]);
    let src = if media {
        &crate::src::MEDIA_ERROR
    } else {
        &crate::src::DEVICE_ERROR
    };
    if let Some((part, record)) = system.lic.as_mut() {
        part.refresh(record);
        record.push_pal(licpart::PalEntry {
            time: now_unix(),
            code: src.bytes(),
            severity: licpart::Severity::Attention,
            resource: rsrc.chars().take(10).collect(),
        });
        let _ = part.write(record);
    }
    let d = prb::analyze::from_hardware(rsrc, src.code, true);
    if let Err(e) = crate::stgsvc::prblog::log(system, PrbType::Machine, &d) {
        eprintln!("qsstg: problem log: {e}");
    }
}

/// What a failed media operation was, for the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    /// Mount, volume, protection and end-of-volume problems.
    Attention,
    /// The device or changer failed.
    Device,
    /// The volume's labels or image are damaged.
    Media,
    /// The command's own refusal; nothing to tell.
    Quiet,
}

/// The event of a medium's error.
pub(crate) fn event_of(e: &medio::MediaError) -> Event {
    use medio::MediaError as M;
    use tape::{TapeError as T, TransportError as X};
    match e {
        M::Tape(T::Transport(X::Io(_) | X::Density(_) | X::BlockTooLarge { .. } | X::BlockSize { .. }) | T::Changer(_)) => Event::Device,
        M::Tape(T::Transport(X::Format(_)) | T::Label(_)) | M::Optical(optical::Error::Malformed(_) | optical::Error::Io(_)) => Event::Media,
        M::Tape(
            T::Transport(_) | T::WrongVolume { .. } | T::NotContinuation { .. } | T::EndOfVolume | T::NotLabeled | T::Labeled,
        )
        | M::Optical(optical::Error::WriteProtected | optical::Error::ReadOnly | optical::Error::VolumeFull { .. }) => Event::Attention,
        _ => Event::Quiet,
    }
}

/// Tell the operator about a failed operation on device `dev` over
/// resource `rsrc`.
pub(crate) fn report(system: &mut System, rsrc: &str, dev: &str, e: &medio::MediaError) {
    let id = e.msgid();
    match event_of(e) {
        Event::Attention => attention(system, id, &[(dev, 10)]),
        Event::Device => device_error(system, rsrc, dev, id, false),
        Event::Media => device_error(system, rsrc, dev, id, true),
        Event::Quiet => {}
    }
}
