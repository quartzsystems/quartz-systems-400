// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message handling APIs: `QMHSNDPM`, `QMHRCVPM`, `QMHRMVPM`,
//! `QMHRTVM`, `QMHSNDM`, `QMHRCVM`, `QMHRMVM`, `QMHSNDBM`, `QMHSNDRM`,
//! `QMHRSNEM`, `QMHMOVPM`, `QMHCHGEM`. Program message queues belong
//! to call-stack entries; the non-program queues this job sees are
//! `QSYSOPR` and its own external queue, named `*EXT`.

use mi::{ClearMode, Job, Message, QueueRef};
use qir::MsgType;

use crate::param::{Args, ErrCode, Out, entry_queue, type_code, type_named};
use crate::provider::{ApiFail, Provider};

/// The APIs of this module.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QMHSNDPM", qmhsndpm),
    ("QMHRCVPM", qmhrcvpm),
    ("QMHRMVPM", qmhrmvpm),
    ("QMHRTVM", qmhrtvm),
    ("QMHSNDM", qmhsndm),
    ("QMHRCVM", qmhrcvm),
    ("QMHRMVM", qmhrmvm),
    ("QMHSNDBM", qmhsndbm),
    ("QMHSNDRM", qmhsndrm),
    ("QMHRSNEM", qmhrsnem),
    ("QMHMOVPM", qmhmovpm),
    ("QMHCHGEM", qmhchgem),
];

fn key_bytes(k: u32) -> [u8; 4] {
    k.to_be_bytes()
}

fn key_of(b: &[u8]) -> u32 {
    u32::from_be_bytes(b.get(..4).and_then(|x| x.try_into().ok()).unwrap_or([0; 4]))
}

/// A message file name with `*LIBL` for a blank library.
fn msgf_parts(a: &Args<'_>, i: usize) -> (String, String) {
    let (f, l) = a.qual(i);
    let l = if l.is_empty() { "*LIBL".into() } else { l };
    (f, l)
}

/// The message data of a (data, length) parameter pair.
fn data_of(a: &Args<'_>, i: usize, len_i: usize) -> Vec<u8> {
    let n = usize::try_from(a.bin4(len_i)).unwrap_or(0);
    a.bytes(i, n)
}

/// A non-program queue by name: `QSYSOPR`, or the job's `*EXT`.
fn named_queue(name: &str) -> Option<QueueRef> {
    match name {
        "QSYSOPR" => Some(QueueRef::SysOpr),
        "*EXT" => Some(QueueRef::Ext),
        _ => None,
    }
}

/// `QMHSNDPM(msgid, msgf, data, len, type, entry, counter, key out,
/// errc)`.
fn qmhsndpm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 8);
    let msgid = a.char(0, 7);
    let (msgf, lib) = msgf_parts(&a, 1);
    let data = data_of(&a, 2, 3);
    let ty = a.char(4, 10);
    let Some(ty) = type_named(&ty).filter(|t| *t != MsgType::Any) else {
        errc.fail(j, &ApiFail::id("CPF24B3").text(&ty, 10));
        return;
    };
    let entry = a.char(5, 10);
    let Some(to) = entry_queue(j, &entry, a.bin4(6)) else {
        errc.fail(j, &ApiFail::id("CPF24A3").bin4(a.bin4(6)));
        return;
    };
    if !msgid.is_empty() && j.msgs.describe(&msgid, &msgf, &lib).is_none() {
        errc.fail(j, &ApiFail::id("CPF2499").text(&msgid, 7));
        return;
    }
    match j.send_message(&msgid, &msgf, &lib, data, ty, to) {
        Ok(key) => {
            a.put_bytes(7, &key_bytes(key));
            errc.ok();
        }
        Err(_) => errc.fail(j, &ApiFail::id("CPF24A3").bin4(a.bin4(6))),
    }
}

/// The `RCVM0100` image of a message, or an empty one.
fn rcvm0100(m: Option<&Message>) -> Out {
    let mut out = Out::new();
    out.zero_to(56);
    let Some(m) = m else {
        out.blank(12, 7);
        out.blank(19, 2);
        out.blank(21, 4);
        out.blank(25, 7);
        return out;
    };
    out.bin4(8, i32::from(m.severity));
    out.char(12, 7, &m.msgid);
    out.char(19, 2, type_code(m.msg_type));
    out.bytes(21, &key_bytes(m.key));
    out.blank(25, 7);
    out.bin4(32, 0);
    out.bin4(36, 37);
    let text = ccsid::to_ebcdic(&m.text).unwrap_or_default();
    let dl = i32::try_from(m.data.len()).unwrap_or(i32::MAX);
    let tl = i32::try_from(text.len()).unwrap_or(i32::MAX);
    out.bin4(40, dl);
    out.bin4(44, dl);
    out.bin4(48, tl);
    out.bin4(52, tl);
    out.bytes(56, &m.data);
    out.bytes(56 + m.data.len(), &text);
    out
}

/// Receive by type or key from a queue with an action: `*SAME` leaves
/// it, `*OLD` marks it received, `*REMOVE` removes it.
fn receive_from(
    j: &mut Job,
    q: QueueRef,
    ty: &str,
    key: &[u8],
    action: &str,
) -> Result<Option<Message>, ApiFail> {
    let (sel_ty, sel_key) = match ty {
        "*NEXT" | "*PRV" => {
            let k = key_of(key);
            let Some(queue) = j.queue(q) else {
                return Ok(None);
            };
            let i = queue.msgs.iter().position(|m| m.key == k);
            let next = match (ty, i) {
                ("*NEXT", Some(i)) => queue.msgs.get(i + 1),
                ("*NEXT", None) => queue.msgs.first(),
                ("*PRV", Some(i)) if i > 0 => queue.msgs.get(i - 1),
                ("*PRV", Some(_)) => None,
                ("*PRV", None) => queue.msgs.last(),
                _ => None,
            };
            match next {
                Some(m) => (None, Some(m.key)),
                None => return Ok(None),
            }
        }
        "*BYKEY" => (None, Some(key_of(key))),
        _ => (
            Some(type_named(ty).ok_or_else(|| ApiFail::id("CPF24B3").text(ty, 10))?),
            None,
        ),
    };
    Ok(match action {
        "*SAME" => j.queue(q).and_then(|queue| {
            queue
                .msgs
                .iter()
                .find(|m| {
                    sel_key.is_none_or(|k| m.key == k)
                        && sel_ty.is_none_or(|t| t == MsgType::Any || m.msg_type == t)
                })
                .cloned()
        }),
        "*REMOVE" => j.receive_message(q, sel_ty, sel_key, true),
        _ => j.receive_message(q, sel_ty, sel_key, false),
    })
}

/// `QMHRCVPM(receiver, len, format, entry, counter, type, key, wait,
/// action, errc)`.
fn qmhrcvpm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 9);
    let format = a.char(2, 8);
    if format != "RCVM0100" {
        errc.fail(j, &ApiFail::id("CPF24B4").text(&format, 8));
        return;
    }
    let entry = a.char(3, 10);
    let Some(q) = entry_queue(j, &entry, a.bin4(4)) else {
        errc.fail(j, &ApiFail::id("CPF24A3").bin4(a.bin4(4)));
        return;
    };
    let ty = a.char(5, 10);
    let key = a.bytes(6, 4);
    let action = a.char(8, 10);
    match receive_from(j, q, &ty, &key, &action) {
        Ok(m) => {
            rcvm0100(m.as_ref()).deliver(&a, 0, 1);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

fn clear_mode(s: &str) -> Option<ClearMode> {
    match s {
        "*ALL" => Some(ClearMode::All),
        "*KEEPUNANS" => Some(ClearMode::KeepUnans),
        "*NEW" => Some(ClearMode::New),
        "*OLD" => Some(ClearMode::Old),
        _ => None,
    }
}

/// `QMHRMVPM(entry, counter, key, which, errc)`.
fn qmhrmvpm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let entry = a.char(0, 10);
    let Some(q) = entry_queue(j, &entry, a.bin4(1)) else {
        errc.fail(j, &ApiFail::id("CPF24A3").bin4(a.bin4(1)));
        return;
    };
    let which = a.char(3, 10);
    let r = if which == "*BYKEY" {
        j.remove_messages(q, Some(key_of(&a.bytes(2, 4))), ClearMode::All)
    } else {
        match clear_mode(&which) {
            Some(m) => j.remove_messages(q, None, m),
            None => {
                errc.fail(j, &ApiFail::id("CPF24B3").text(&which, 10));
                return;
            }
        }
    };
    match r {
        Ok(_) => errc.ok(),
        Err(_) => errc.fail(j, &ApiFail::id("CPF24A3").bin4(a.bin4(1))),
    }
}

/// `QMHRTVM(receiver, len, format, msgid, msgf, data, len, replace,
/// return format control, errc)`: `RTVM0100`.
fn qmhrtvm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 9);
    let format = a.char(2, 8);
    if format != "RTVM0100" {
        errc.fail(j, &ApiFail::id("CPF24B4").text(&format, 8));
        return;
    }
    let msgid = a.char(3, 7);
    let (msgf, lib) = msgf_parts(&a, 4);
    let data = data_of(&a, 5, 6);
    let replace = a.char(7, 10) != "*NO";
    let Some(d) = j.msgs.describe(&msgid, &msgf, &lib) else {
        errc.fail(
            j,
            &ApiFail::id("CPF2419")
                .text(&msgid, 7)
                .text(&msgf, 10)
                .text(&lib, 10),
        );
        return;
    };
    let (text, help) = if replace {
        (d.format(&data), d.format_help(&data))
    } else {
        (d.text.clone(), d.help.clone())
    };
    let text = ccsid::to_ebcdic(&text).unwrap_or_default();
    let help = ccsid::to_ebcdic(&help).unwrap_or_default();
    let mut out = Out::new();
    let tl = i32::try_from(text.len()).unwrap_or(i32::MAX);
    let hl = i32::try_from(help.len()).unwrap_or(i32::MAX);
    out.bin4(8, tl);
    out.bin4(12, tl);
    out.bin4(16, hl);
    out.bin4(20, hl);
    out.bytes(24, &text);
    out.bytes(24 + text.len(), &help);
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QMHSNDM(msgid, msgf, data, len, type, queues, count, reply queue,
/// key out, errc)`: to `QSYSOPR` or `*EXT`.
fn qmhsndm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 9);
    let msgid = a.char(0, 7);
    let (msgf, lib) = msgf_parts(&a, 1);
    let data = data_of(&a, 2, 3);
    let ty = a.char(4, 10);
    let Some(ty) = type_named(&ty).filter(|t| *t != MsgType::Any) else {
        errc.fail(j, &ApiFail::id("CPF24B3").text(&ty, 10));
        return;
    };
    let n = usize::try_from(a.bin4(6)).unwrap_or(0).max(1);
    let names = a.bytes(5, 20 * n);
    let mut last = 0;
    for q in names.chunks(20) {
        let name = ccsid::decode_trimmed(&q[..10]);
        let Some(q) = named_queue(&name) else {
            errc.fail(j, &ApiFail::id("CPF2403").text(&name, 10));
            return;
        };
        match j.send_message(&msgid, &msgf, &lib, data.clone(), ty, q) {
            Ok(k) => last = k,
            Err(_) => {
                errc.fail(j, &ApiFail::id("CPF2403").text(&name, 10));
                return;
            }
        }
    }
    a.put_bytes(8, &key_bytes(last));
    errc.ok();
}

/// `QMHRCVM(receiver, len, format, queue, type, key, wait, action,
/// errc)`.
fn qmhrcvm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 8);
    let format = a.char(2, 8);
    if format != "RCVM0100" {
        errc.fail(j, &ApiFail::id("CPF24B4").text(&format, 8));
        return;
    }
    let (name, _) = a.qual(3);
    let Some(q) = named_queue(&name) else {
        errc.fail(j, &ApiFail::id("CPF2403").text(&name, 10));
        return;
    };
    let ty = a.char(4, 10);
    let key = a.bytes(5, 4);
    let action = a.char(7, 10);
    match receive_from(j, q, &ty, &key, &action) {
        Ok(m) => {
            rcvm0100(m.as_ref()).deliver(&a, 0, 1);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QMHRMVM(queue, key, which, errc)`.
fn qmhrmvm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let (name, _) = a.qual(0);
    let Some(q) = named_queue(&name) else {
        errc.fail(j, &ApiFail::id("CPF2403").text(&name, 10));
        return;
    };
    let which = a.char(2, 10);
    let r = if which == "*BYKEY" {
        j.remove_messages(q, Some(key_of(&a.bytes(1, 4))), ClearMode::All)
    } else {
        match clear_mode(&which) {
            Some(m) => j.remove_messages(q, None, m),
            None => {
                errc.fail(j, &ApiFail::id("CPF24B3").text(&which, 10));
                return;
            }
        }
    };
    let _ = r;
    errc.ok();
}

/// `QMHSNDBM(text, len, queue, errc)`: a break message to this job's
/// external queue.
fn qmhsndbm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let data = data_of(&a, 0, 1);
    let _ = j.send_message("", "", "", data, MsgType::Info, QueueRef::Ext);
    errc.ok();
}

/// `QMHSNDRM(key, queue, reply, len, remove, errc)`: answer an
/// inquiry on `*EXT` or `QSYSOPR`.
fn qmhsndrm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let key = key_of(&a.bytes(0, 4));
    let (name, _) = a.qual(1);
    let Some(q) = named_queue(&name) else {
        errc.fail(j, &ApiFail::id("CPF2403").text(&name, 10));
        return;
    };
    let reply = ccsid::decode_trimmed(&data_of(&a, 2, 3));
    let remove = a.char(4, 10) == "*YES";
    let queue = match q {
        QueueRef::Ext => &mut j.ext_msgq,
        _ => &mut j.sysopr,
    };
    let Some(m) = queue
        .msgs
        .iter_mut()
        .find(|m| m.key == key && m.msg_type == MsgType::Inq)
    else {
        errc.fail(
            j,
            &ApiFail::id("CPF2410").bin4(i32::try_from(key).unwrap_or(0)),
        );
        return;
    };
    m.reply = Some(reply.clone());
    if remove {
        queue.remove(key);
    }
    j.inquiry_log.push(reply);
    errc.ok();
}

/// `QMHRSNEM(key, errc, entry, counter)`: resend an escape from the
/// caller's queue to the entry above (its caller by default).
fn qmhrsnem(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 1);
    let key = key_of(&a.bytes(0, 4));
    let Some(m) = j
        .queue(QueueRef::Same)
        .and_then(|q| q.msgs.iter().find(|m| m.key == key))
        .cloned()
    else {
        errc.fail(
            j,
            &ApiFail::id("CPF2410").bin4(i32::try_from(key).unwrap_or(0)),
        );
        return;
    };
    let entry = if a.present(2) {
        a.char(2, 10)
    } else {
        "*".into()
    };
    let counter = if a.present(3) { a.bin4(3) } else { 1 };
    let Some(to) = entry_queue(j, &entry, counter) else {
        errc.fail(j, &ApiFail::id("CPF24A3").bin4(counter));
        return;
    };
    errc.ok();
    let _ = j.send_message(
        &m.msgid,
        &m.msgf,
        &m.msgf_lib,
        m.data.clone(),
        MsgType::Escape,
        to,
    );
}

/// `QMHMOVPM(key, types, count, entry, counter, errc)`: move messages
/// of the types from the caller's queue to another entry's.
fn qmhmovpm(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let n = usize::try_from(a.bin4(2)).unwrap_or(0);
    let types: Vec<MsgType> = a
        .bytes(1, 10 * n)
        .chunks(10)
        .filter_map(|c| type_named(&ccsid::decode_trimmed(c)))
        .collect();
    let entry = a.char(3, 10);
    let Some(to) = entry_queue(j, &entry, a.bin4(4)) else {
        errc.fail(j, &ApiFail::id("CPF24A3").bin4(a.bin4(4)));
        return;
    };
    let keys: Vec<u32> = j
        .queue(QueueRef::Same)
        .map(|q| {
            q.msgs
                .iter()
                .filter(|m| types.is_empty() || types.contains(&m.msg_type))
                .map(|m| m.key)
                .collect()
        })
        .unwrap_or_default();
    for k in keys {
        j.move_message(k, QueueRef::Same, to);
    }
    errc.ok();
}

/// `QMHCHGEM(entry, counter, key, action, data, len, errc)`:
/// `*HANDLE` marks the exception handled; `*CANCEL` and `*TRANSFER`
/// are accepted and do nothing more here.
fn qmhchgem(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 6);
    let key = key_of(&a.bytes(2, 4));
    let action = a.char(3, 10);
    match action.as_str() {
        "*HANDLE" => {
            if j.handle_pending(key) {
                errc.ok();
            } else {
                errc.fail(
                    j,
                    &ApiFail::id("CPF2410").bin4(i32::try_from(key).unwrap_or(0)),
                );
            }
        }
        "*CANCEL" | "*TRANSFER" => errc.ok(),
        _ => errc.fail(j, &ApiFail::id("CPF24B3").text(&action, 10)),
    }
}
