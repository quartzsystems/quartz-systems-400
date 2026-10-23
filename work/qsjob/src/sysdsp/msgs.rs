// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages that interrupt the operator. Break messages on a queue in
//! `*BREAK` delivery (the user's, `QSYSOPR`'s at the console) come up
//! on the Display Messages panel of their queue, the screen put back
//! after. An inquiry a program sends to the external queue (`*EXT`,
//! the job's `INQMSGRPY(*RQD)`, a function check's `CEE9901`) comes up
//! on Display Program Messages, whose reply goes back to the program.

use mi::job::Inquiry;
use stg::Client;
use tn5250::ds::{aid, attr, field::Ffw};
use tn5250::screen::{Read, ScreenModel};
use wm::obj::msgq::QMsg;

use super::keys;
use crate::command::RunMode;
use crate::console::host::Host;
use crate::display::surface::Surface;

/// Take the breaks waiting for a job that has no runtime job at hand
/// (a display file or panel is waiting): at the System Request level.
pub fn poll(host: &Host) {
    let waiting = host.link.borrow_mut().with(Client::breaks);
    if waiting.is_empty() {
        return;
    }
    if waiting.iter().all(|(_, (pgm, _), _)| pgm.is_empty() || pgm.starts_with('*')) {
        breaks(host, waiting.into_iter().map(|(q, _, m)| (q, m)).collect());
        return;
    }
    keys::with_level(host, |job| host.deliver(job, waiting));
}

/// Show break messages: each queue's Display Messages, the screen put
/// back after.
pub fn breaks(host: &Host, msgs: Vec<(String, QMsg)>) {
    if keys::unwinding() {
        return;
    }
    let mut queues: Vec<String> = Vec::new();
    for (q, _) in msgs {
        if !queues.contains(&q) {
            queues.push(q);
        }
    }
    let saved = host.display.borrow().as_ref().map(crate::display::Display::snapshot);
    for q in queues {
        let q = if q.contains('/') { q } else { format!("*LIBL/{q}") };
        let _ = keys::command(host, &format!("DSPMSG MSGQ({q})"), RunMode::Run);
        if keys::unwinding() {
            return;
        }
    }
    keys::put_back(host, saved, None);
}

/// The reply to an inquiry by the job's `INQMSGRPY`: `*DFT` takes the
/// default, `*SYSRPYL` the reply list's entry; otherwise the operator
/// at the job's display answers on Display Program Messages (a job
/// without one asks `QSYSOPR`). `None` is the default reply.
pub fn inquiry(host: &Host, q: &Inquiry) -> Option<String> {
    if host.display.borrow().is_none() {
        return stg::remote::inquiry::answer(&host.link, q);
    }
    let mode = host.link.borrow_mut().with(Client::rtvjoba).inqmsgrpy;
    match mode.as_str() {
        "*DFT" => return None,
        "*SYSRPYL" => {
            let found = host
                .link
                .borrow_mut()
                .with(|l| l.sysrpyl(q.msgid.clone(), q.data.clone()));
            match found {
                Some((r, _)) if r == "*DFT" => return None,
                Some((r, _)) if r != "*RQD" => return Some(r),
                _ => {}
            }
        }
        _ => {}
    }
    ask(host, q)
}

/// The rows of Display Program Messages.
pub mod rows {
    /// The title.
    pub const TITLE: usize = 1;
    /// The first message line.
    pub const FIRST: usize = 3;
    /// The last message line.
    pub const LAST: usize = 17;
    /// The reply instruction.
    pub const TYPE_REPLY: usize = 19;
    /// The reply field's row.
    pub const REPLY: usize = 20;
    /// The function keys.
    pub const KEYS: usize = 23;
}

/// Lay Display Program Messages out on a model: the message lines,
/// and with a reply the instruction and the field (its index).
pub fn draw(m: &mut ScreenModel, lines: &[String], reply: Option<&str>, message: &str) -> Option<usize> {
    m.clear();
    let title = "Display Program Messages";
    let cols = m.cols();
    m.put(rows::TITLE, 1 + (cols - title.len()) / 2, title, attr::WHITE);
    for (i, l) in lines.iter().take(rows::LAST - rows::FIRST + 1).enumerate() {
        let shown: String = l.chars().take(cols - 3).collect();
        m.put(rows::FIRST + i, 2, &shown, attr::GREEN);
    }
    let mut field = None;
    if let Some(v) = reply {
        m.put(rows::TYPE_REPLY, 2, "Type reply, press Enter.", attr::BLUE);
        m.put(rows::REPLY, 4, "Reply . . .", attr::GREEN);
        let len = cols - 18;
        m.field(rows::REPLY, 17, len, attr::GREEN_UL, v, Ffw::default(), &[]);
        field = m.field_index_at(rows::REPLY, 17);
    }
    m.put(rows::KEYS, 2, "F3=Exit   F12=Cancel", attr::BLUE);
    m.set_error_row(m.rows());
    if !message.is_empty() {
        m.error_line(message, attr::WHITE);
        m.alarm();
    }
    field
}

/// An inquiry's text as the display shows it: the sender, then the
/// text wrapped at 74 columns, indented.
#[must_use]
pub fn inquiry_lines(from: &str, text: &str) -> Vec<String> {
    let mut out = vec![format!("From  . . . :   {from}"), String::new()];
    let mut line = String::new();
    for w in text.split_whitespace() {
        if line.len() + w.len() + 1 > 72 && !line.is_empty() {
            out.push(format!("  {line}"));
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(w);
    }
    if !line.is_empty() {
        out.push(format!("  {line}"));
    }
    out
}

/// Ask the operator; the reply, `None` for the default.
fn ask(host: &Host, q: &Inquiry) -> Option<String> {
    let saved = host.display.borrow().as_ref().map(crate::display::Display::snapshot);
    let mut surface = Surface::new(host.clone());
    let text = format!("{} {}", q.msgid, q.text.trim()).trim().to_string();
    let lines = inquiry_lines(&host.start.user, &text);
    let mut message = String::new();
    let mut typed = String::new();
    let reply = loop {
        let field = surface.with_model(|m| draw(m, &lines, Some(&typed), &message));
        surface.flush(Read::MdtFields);
        let Some((a, _)) = surface.wait() else {
            break None;
        };
        typed = surface.with_model(|m| {
            field
                .and_then(|i| m.fields().get(i).map(tn5250::screen::Field::text))
                .unwrap_or_default()
        });
        message.clear();
        match a {
            aid::ENTER => {}
            a if matches!(aid::function_of(a), Some(3 | 12)) => break None,
            _ => {
                message = "Function key not allowed.".into();
                continue;
            }
        }
        let r = typed.trim().to_ascii_uppercase();
        if r.is_empty() {
            break None;
        }
        if !q.valid.is_empty() && !q.valid.iter().any(|v| v.eq_ignore_ascii_case(&r)) {
            message = format!("Reply {r} is not valid; type one of {}.", q.valid.join(", "));
            continue;
        }
        break Some(r);
    };
    keys::put_back(host, saved, None);
    reply
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_messages_layout() {
        let mut m = ScreenModel::new(false);
        let lines = inquiry_lines("QPGMR", "CEE9901 Something went wrong in a program.");
        let f = draw(&mut m, &lines, Some(""), "").unwrap();
        let rows = m.text_rows();
        assert_eq!(rows[0].trim(), "Display Program Messages");
        assert!(rows[rows::FIRST - 1].contains("QPGMR"));
        assert!(rows[rows::FIRST + 1].contains("CEE9901"));
        assert!(rows[rows::REPLY - 1].contains("Reply"));
        assert_eq!(m.fields()[f].col, 17);
    }
}
