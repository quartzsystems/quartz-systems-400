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
        from_pgm: "QTAERR".into(),
        ..QMsg::default()
    }
}

/// Ask the operator; nobody answers at once here, so the default reply
/// is taken and the message shows as answered with it. The reply.
pub(crate) fn inquiry(system: &mut System, msgid: &str, data: &[(&str, usize)], dft: &str) -> String {
    let mut m = message(system, msgid, data);
    m.kind = MsgKind::Inq;
    m.severity = 99;
    m.status = Status::Answered;
    m.dft = dft.into();
    m.reply = dft.into();
    send(system, m);
    dft.to_string()
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
