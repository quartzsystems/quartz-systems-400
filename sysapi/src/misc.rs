// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commands, security, commitment control and the stubs: `QCMDEXC`,
//! `QCAPCMD`, `QCMDCHK`, `QSYRUSRI`, `QSYCUSRA`, `QTNRCMTI`,
//! `QUSLSPL`, `QWDRSBSD`, `QWCRJBST`. The command APIs hand the text
//! to the provider (`MIGRATE(phase7)`: the command analyzer).

use mi::Job;

use crate::param::{Args, ErrCode, Out, fixed};
use crate::provider::{ApiFail, Provider};

/// The APIs of this module.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QCMDEXC", qcmdexc),
    ("QCAPCMD", qcapcmd),
    ("QCMDCHK", qcmdchk),
    ("QSYRUSRI", qsyrusri),
    ("QSYCUSRA", qsycusra),
    ("QTNRCMTI", qtnrcmti),
    ("QUSLSPL", quslspl),
    ("QWDRSBSD", qwdrsbsd),
    ("QWCRJBST", qwcrjbst),
];

/// The command text of a (text, packed(15,5) length) pair.
fn command_text(a: &Args<'_>) -> String {
    let n = usize::try_from(a.packed(1, 15, 5)).unwrap_or(0);
    ccsid::decode_trimmed(&a.bytes(0, n))
}

/// A failure as an escape to the caller, the way the command APIs
/// report (they have no error code parameter).
fn escape(j: &mut Job, f: &ApiFail) {
    let _ = j.send_message(
        &f.msgid,
        crate::param::file_of(&f.msgid),
        "QSYS",
        f.data.clone(),
        qir::MsgType::Escape,
        mi::QueueRef::Same,
    );
}

/// `QCMDEXC(command, length, [igc])`.
fn qcmdexc(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let text = command_text(&a);
    if let Err(f) = p.exec_command(&text) {
        escape(j, &f);
    }
}

/// `QCMDCHK(command, length)`.
fn qcmdchk(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let text = command_text(&a);
    if let Err(f) = p.check_command(&text) {
        escape(j, &f);
    }
}

/// `QCAPCMD(source, length, options, options length, options format,
/// changed command, changed length, available length, errc)`:
/// `CPOP0100`, whose type of command processing is 0 (run), 1 (check
/// syntax) or 2 (prompt and run, treated as run).
fn qcapcmd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 8);
    let n = usize::try_from(a.bin4(1)).unwrap_or(0);
    let text = ccsid::decode_trimmed(&a.bytes(0, n));
    let format = a.char(4, 8);
    if format != "CPOP0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let opts = a.bytes(2, 20);
    let kind = crate::param::bin4_at(&opts, 0);
    let r = match kind {
        1 => p.check_command(&text),
        _ => p.exec_command(&text),
    };
    match r {
        Ok(()) => {
            let changed = fixed(&text, text.len());
            let room = usize::try_from(a.bin4(6)).unwrap_or(0);
            a.put_bin4(7, i32::try_from(changed.len()).unwrap_or(i32::MAX));
            let n = room.min(changed.len());
            if n > 0 {
                a.put_bytes(5, &changed[..n]);
            }
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QSYRUSRI(receiver, len, format, user, errc)`: `USRI0100`.
fn qsyrusri(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(2, 8);
    if !matches!(format.as_str(), "USRI0100" | "USRI0200" | "USRI0300") {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let mut name = a.char(3, 10);
    if name == "*CURRENT" || name.is_empty() {
        name.clone_from(&j.ident.user);
    }
    let Some(u) = p.user_info(&name) else {
        errc.fail(j, &ApiFail::id("CPF9802").text(&name, 10));
        return;
    };
    let mut out = Out::new();
    out.char(8, 10, &u.name);
    out.blank(18, 13);
    out.blank(31, 1);
    out.bin4(32, 0);
    out.char(36, 10, &u.status);
    out.blank(46, 8);
    out.ch(54, 'N');
    out.blank(55, 1);
    out.bin4(56, 0);
    out.blank(60, 8);
    out.bin4(68, 0);
    out.ch(72, '0');
    out.char(73, 10, "*SYSVAL");
    out.ch(83, '0');
    out.char(84, 10, "*SYSVAL");
    if format != "USRI0100" {
        out.char(94, 10, &u.class);
        let mut at = 104;
        for s in &u.special {
            out.char(at, 10, s);
            at += 10;
        }
        out.pad_to(184);
        out.char(184, 50, &u.text);
    }
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QSYCUSRA(indicator, format, object, type, authorities, count,
/// user, errc)`: always authorized here.
fn qsycusra(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 7);
    let format = a.char(1, 8);
    if format != "CHKA0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    a.put_char(0, 1, "Y");
    errc.ok();
}

/// `QTNRCMTI(receiver, len, format, errc)`: `CMTI0100`.
fn qtnrcmti(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let format = a.char(2, 8);
    if format != "CMTI0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let Some(c) = p.commit_info() else {
        errc.fail(j, &ApiFail::id("CPF83E4"));
        return;
    };
    let mut out = Out::new();
    out.char(8, 10, "*DFTACTGRP");
    out.char(18, 10, &c.lock_level);
    out.ch(28, if c.pending { '1' } else { '0' });
    let (jn, jl) = c.journal.unwrap_or_default();
    out.char(29, 10, &jn);
    out.char(39, 10, &jl);
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QUSLSPL(user space, format, user, output queue, form type, user
/// data, errc)`: an empty list.
fn quslspl(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 6);
    let format = a.char(1, 8);
    let size = match format.as_str() {
        "SPLF0100" => 88,
        "SPLF0200" => 176,
        _ => {
            errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
            return;
        }
    };
    let input = [
        a.bytes(0, 20),
        fixed(&format, 8),
        a.bytes(2, 10),
        a.bytes(3, 20),
        a.bytes(4, 10),
        a.bytes(5, 10),
    ]
    .concat();
    match crate::obj::write_list(j, &a, 0, &format, "QUSLSPL", &input, &[], &[], size) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QWDRSBSD(receiver, len, format, subsystem, errc)`: `SBSI0100`,
/// every subsystem inactive.
fn qwdrsbsd(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(2, 8);
    if format != "SBSI0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (name, lib) = a.qual(3);
    let mut out = Out::new();
    out.char(8, 10, &name);
    out.char(18, 10, if lib.is_empty() { "QSYS" } else { &lib });
    out.char(28, 10, "*INACTIVE");
    out.bin4(38, 0);
    out.bin4(42, 0);
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QWCRJBST(receiver, len, format, job, internal id, errc)`:
/// `JBST0100`, `*ACTIVE` for this job.
fn qwcrjbst(j: &mut Job, args: &[u64], _p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let format = a.char(2, 8);
    if format != "JBST0100" {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let qual = a.bytes(3, 26);
    let name = ccsid::decode_trimmed(&qual[..10]);
    let mine = name == "*" || name == j.ident.name;
    if !mine {
        errc.fail(
            j,
            &ApiFail::id("CPF3C53")
                .text(&name, 10)
                .text(&ccsid::decode_trimmed(&qual[10..20]), 10)
                .text(&ccsid::decode_trimmed(&qual[20..26]), 6),
        );
        return;
    }
    let mut out = Out::new();
    out.char(8, 10, "*ACTIVE");
    out.deliver(&a, 0, 1);
    errc.ok();
}
