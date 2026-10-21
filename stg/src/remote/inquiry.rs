// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Answering an inquiry by the job's `INQMSGRPY`: `*DFT` takes the
//! default reply, `*SYSRPYL` the system reply list's (falling back to
//! `*RQD` when no entry matches or the entry says `*RQD`), and `*RQD`
//! sends the inquiry to `QSYSOPR` and waits, in `MSGW`, for the
//! operator's reply.

use std::time::Duration;

use mi::job::Inquiry;
use wm::job::Status;
use wm::obj::msgq::{MsgKind, QMsg};

use super::status;
use crate::Shared;
use crate::proto::Client;

/// How often a job waiting for a reply looks.
const POLL: Duration = Duration::from_millis(250);

/// The reply, or `None` for the message's default.
pub fn answer(link: &Shared, q: &Inquiry) -> Option<String> {
    let mode = link.borrow_mut().with(Client::rtvjoba).inqmsgrpy;
    match mode.as_str() {
        "*DFT" => None,
        "*SYSRPYL" => {
            let found = link
                .borrow_mut()
                .with(|l| l.sysrpyl(q.msgid.clone(), q.data.clone()));
            match found {
                Some((r, _)) if r == "*DFT" => None,
                Some((r, _)) if r != "*RQD" => Some(r),
                _ => ask_operator(link, q),
            }
        }
        _ => ask_operator(link, q),
    }
}

/// Send the inquiry to `QSYSOPR` and wait for its reply.
pub fn ask_operator(link: &Shared, q: &Inquiry) -> Option<String> {
    let m = QMsg {
        kind: MsgKind::Inq,
        severity: 99,
        msgid: q.msgid.clone(),
        msgf: (q.msgf.clone(), "QSYS".into()),
        data: q.data.clone(),
        text: q.text.clone(),
        help: q.help.clone(),
        dft: q.dft.clone().unwrap_or_default(),
        reply_to: ("QSYSOPR".into(), "QSYS".into()),
        ..QMsg::default()
    };
    let key = link
        .borrow_mut()
        .with(|l| l.msgq_send("*SYSOPR".into(), m))
        .ok()?;
    status(link, Status::Msgw, "MSGW-QSYSOPR");
    let reply = loop {
        std::thread::sleep(POLL);
        let Ok(msgs) = link.borrow_mut().with(|l| l.msgq_list("*SYSOPR".into())) else {
            break None;
        };
        match msgs.iter().find(|m| m.key == key) {
            // Removed unanswered: the default.
            None => break None,
            Some(m) if m.status == wm::obj::msgq::Status::Answered => {
                break Some(m.reply.clone());
            }
            Some(_) => {}
        }
    };
    status(link, Status::Run, "");
    reply
}
