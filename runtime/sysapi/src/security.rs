// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security APIs: `QSYRUSRI`, `QSYCUSRS`, `QSYCUSRA`, `QSYRUSRA`,
//! `QSYLOBJA`, `QSYLOBJP`, `QSYRAUTU`, `QSYLAUTU`, `QSYLATLO`,
//! `QSYGETPH`, `QWTSETP`, `QSYRLSPH`, `QSYCHGPW`, `QSYRUPWD` and
//! `QSYSUPWD`. Each asks the provider one `sec::api::Query`; the
//! receivers follow IBM's field order, laid out as `docs/SYSAPI.md`
//! says.

use mi::Job;
use sec::api::{Answer, ObjRow, Query, UserRow};
use sec::profile::{SpecialAuth, UsrPrf};

use crate::param::{Args, ErrCode, Out, fixed};
use crate::provider::{ApiFail, Provider};

/// The APIs of this module.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QSYRUSRI", qsyrusri),
    ("QSYCUSRS", qsycusrs),
    ("QSYCUSRA", qsycusra),
    ("QSYRUSRA", qsyrusra),
    ("QSYLOBJA", qsylobja),
    ("QSYLOBJP", qsylobjp),
    ("QSYRAUTU", qsyrautu),
    ("QSYLAUTU", qsylautu),
    ("QSYLATLO", qsylatlo),
    ("QSYGETPH", qsygetph),
    ("QWTSETP", qwtsetp),
    ("QSYRLSPH", qsyrlsph),
    ("QSYCHGPW", qsychgpw),
    ("QSYRUPWD", qsyrupwd),
    ("QSYSUPWD", qsysupwd),
];

fn bad_format(format: &str) -> ApiFail {
    ApiFail::id("CPF3C21").text(format, 8)
}

fn yn(b: bool) -> char {
    if b { 'Y' } else { 'N' }
}

/// The eight special authorities as `Y`/`N` flags and seven reserved.
fn special_flags(s: SpecialAuth) -> String {
    let mut out: String = SpecialAuth::NAMES
        .iter()
        .map(|(_, a)| yn(s.has(*a)))
        .collect();
    out.push_str("NNNNNNN");
    out
}

/// The object and data authority flags of a set, in IBM's order:
/// `*OBJOPR *OBJMGT *OBJEXIST *OBJALTER *OBJREF`, a reserved byte,
/// `*READ *ADD *UPD *DLT *EXECUTE`.
fn auth_flags(bits: u32) -> String {
    let b = |m: u32| yn(bits & m != 0);
    [
        b(0x01),
        b(0x02),
        b(0x04),
        b(0x08),
        b(0x10),
        'N',
        b(0x20),
        b(0x40),
        b(0x80),
        b(0x100),
        b(0x200),
    ]
    .iter()
    .collect()
}

fn auth_name(bits: u32, autl: bool) -> &'static str {
    sec::auth::bits::Auth(bits & sec::auth::bits::Auth::MASK).display(autl)
}

fn ask(j: &mut Job, p: &mut dyn Provider, errc: ErrCode, q: Query) -> Option<Answer> {
    match p.sec(q) {
        Ok(a) => Some(a),
        Err(f) => {
            errc.fail(j, &f);
            None
        }
    }
}

fn user_or_current(j: &Job, name: String) -> String {
    if name.is_empty() || name == "*CURRENT" {
        j.ident.user.clone()
    } else {
        name
    }
}

/// `QSYRUSRI(receiver, len, format, user, errc)`: `USRI0100`
/// (sign-on and password), `USRI0200` (class, special authorities,
/// groups), `USRI0300` (everything).
fn qsyrusri(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(2, 8);
    if !matches!(format.as_str(), "USRI0100" | "USRI0200" | "USRI0300") {
        errc.fail(j, &bad_format(&format));
        return;
    }
    let name = user_or_current(j, a.char(3, 10));
    let Some(Answer::Profile { data }) = ask(j, p, errc, Query::Profile { user: name }) else {
        return;
    };
    let Ok(u) = UsrPrf::decode(&data) else {
        errc.fail(j, &ApiFail::id("CPF2204"));
        return;
    };
    let mut out = Out::new();
    out.char(8, 10, &u.name);
    match format.as_str() {
        "USRI0100" => usri0100(&mut out, &u),
        "USRI0200" => usri0200(&mut out, &u, 18),
        _ => {
            usri0100(&mut out, &u);
            usri0200(&mut out, &u, 94);
            usri0300(&mut out, &u);
        }
    }
    out.deliver(&a, 0, 1);
    errc.ok();
}

fn ts(t: u64) -> String {
    if t == 0 {
        String::new()
    } else {
        crate::job::timestamp13(t)
    }
}

fn interval(v: u32) -> i32 {
    match v {
        0 => 0,
        u32::MAX => -1,
        n => i32::try_from(n).unwrap_or(i32::MAX),
    }
}

fn usri0100(out: &mut Out, u: &UsrPrf) {
    out.char(18, 13, &ts(u.prev_signon));
    out.blank(31, 1);
    out.bin4(32, i32::try_from(u.signon_invalid).unwrap_or(i32::MAX));
    out.char(36, 10, if u.disabled { "*DISABLED" } else { "*ENABLED" });
    out.char(
        46,
        8,
        &ts(u.pwd_changed).chars().take(8).collect::<String>(),
    );
    out.ch(54, yn(u.pwd_none));
    out.blank(55, 1);
    out.bin4(56, interval(u.pwdexpitv));
    out.blank(60, 8);
    out.bin4(68, -1);
    out.ch(72, yn(u.pwdexp));
    out.char(73, 10, u.dspsgninf.name());
    out.ch(83, yn(u.lclpwdmgt));
    out.char(
        84,
        10,
        &match u.pwdchgblk {
            0 => "*SYSVAL".to_string(),
            u32::MAX => "*NONE".to_string(),
            n => n.to_string(),
        },
    );
}

/// The `USRI0200` part from `at`: class, special authorities, groups.
fn usri0200(out: &mut Out, u: &UsrPrf, at: usize) {
    out.char(at, 10, u.usrcls.name());
    out.char(at + 10, 15, &special_flags(u.spcaut));
    out.char(at + 25, 10, &u.grpprf);
    out.char(
        at + 35,
        10,
        if u.owner_grpprf { "*GRPPRF" } else { "*USRPRF" },
    );
    out.char(at + 45, 10, &u.grpaut);
    out.char(at + 55, 10, u.lmtcpb.name());
    out.char(
        at + 65,
        10,
        if u.grpauttyp_pgp { "*PGP" } else { "*PRIVATE" },
    );
    out.blank(at + 75, 3);
    out.bin4(at + 78, 0);
    out.bin4(at + 82, i32::try_from(u.supgrpprf.len()).unwrap_or(0));
    out.char(at + 86, 50, &u.text);
    let groups_at = at + 136;
    for (i, g) in u.supgrpprf.iter().enumerate() {
        out.char(groups_at + i * 10, 10, g);
    }
    if !u.supgrpprf.is_empty() {
        out.bin4(at + 78, i32::try_from(groups_at).unwrap_or(0));
    }
}

/// The rest of `USRI0300`, after the groups.
fn usri0300(out: &mut Out, u: &UsrPrf) {
    let at = 94 + 136 + 150;
    out.pad_to(at);
    let q = |(n, l): &(String, String)| (n.clone(), l.clone());
    let pairs = [
        ("CURLIB", (u.curlib.clone(), String::new())),
        ("INLPGM", q(&u.inlpgm)),
        ("INLMNU", q(&u.inlmnu)),
        ("JOBD", q(&u.jobd)),
        ("MSGQ", q(&u.msgq)),
        ("OUTQ", q(&u.outq)),
        ("ATNPGM", q(&u.atnpgm)),
    ];
    let mut off = at;
    for (_, (n, l)) in pairs {
        out.char(off, 10, &n);
        out.char(off + 10, 10, &l);
        off += 20;
    }
    out.char(off, 15, &u.acgcde);
    out.char(off + 15, 10, &u.dlvry);
    out.bin4(off + 25, i32::from(u.sev));
    out.char(off + 29, 10, &u.prtdev);
    out.char(off + 39, 10, &u.astlvl);
    out.char(off + 49, 10, &u.spcenv);
    out.char(off + 59, 10, &u.kbdbuf);
    out.char(off + 69, 10, &u.langid);
    out.char(off + 79, 10, &u.cntryid);
    out.bin4(off + 89, i32::try_from(u.ccsid).unwrap_or(0));
    out.bin4(off + 93, i32::try_from(u.uid).unwrap_or(0));
    out.bin4(off + 97, i32::try_from(u.gid).unwrap_or(0));
    out.bin4(off + 101, i32::try_from(u.maxstg).unwrap_or(-1));
    out.bin4(off + 105, i32::try_from(u.stg_used).unwrap_or(i32::MAX));
    out.ch(off + 109, char::from(b'0' + u.ptylmt.min(9)));
    out.char(off + 110, 10, &u.objaud);
    out.char(
        off + 120,
        10,
        &ts(u.usrexpdate).chars().take(8).collect::<String>(),
    );
    out.char(off + 130, 64, &u.homedir);
}

/// `QSYCUSRS(indicator, user, special authorities, count, call level,
/// errc)`: `Y` when the user has every one named.
fn qsycusrs(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let user = user_or_current(j, a.char(1, 10));
    let n = usize::try_from(a.bin4(3)).unwrap_or(0).min(8);
    let raw = a.bytes(2, 10 * n);
    let auths: Vec<String> = raw.chunks(10).map(ccsid::decode_trimmed).collect();
    if let Some(Answer::Yes { yes }) = ask(j, p, errc, Query::Special { user, auths }) {
        a.put_char(0, 1, if yes { "Y" } else { "N" });
        errc.ok();
    }
}

/// `QSYCUSRA(indicator, user, object, type, authorities, count, call
/// level, errc)`: `Y` when the user has every authority named.
fn qsycusra(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 7);
    let user = user_or_current(j, a.char(1, 10));
    let (name, lib) = a.qual(2);
    let ty = a.char(3, 10);
    let n = usize::try_from(a.bin4(5)).unwrap_or(0).min(16);
    let raw = a.bytes(4, 10 * n);
    let auths: Vec<String> = raw.chunks(10).map(ccsid::decode_trimmed).collect();
    let q = Query::Check {
        user,
        object: (name, lib, ty),
        auths,
    };
    if let Some(Answer::Yes { yes }) = ask(j, p, errc, q) {
        a.put_char(0, 1, if yes { "Y" } else { "N" });
        errc.ok();
    }
}

/// `QSYRUSRA(receiver, len, format, user, object, type, errc)`:
/// `USRA0100`, what the user has and where it came from.
fn qsyrusra(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 6);
    let format = a.char(2, 8);
    if format != "USRA0100" {
        errc.fail(j, &bad_format(&format));
        return;
    }
    let user = user_or_current(j, a.char(3, 10));
    let (name, lib) = a.qual(4);
    let ty = a.char(5, 10);
    let q = Query::UserAuth {
        user,
        object: (name, lib, ty),
    };
    let Some(Answer::Auth { info }) = ask(j, p, errc, q) else {
        return;
    };
    let mut out = Out::new();
    out.char(8, 10, auth_name(info.bits, info.autl));
    out.ch(18, yn(info.bits & 0x400 != 0));
    out.char(19, 11, &auth_flags(info.bits));
    out.char(30, 16, &info.source);
    out.char(46, 10, &info.list);
    out.char(56, 10, &info.owner);
    out.char(66, 10, &info.pgp);
    out.deliver(&a, 0, 1);
    errc.ok();
}

fn obj_entry(r: &ObjRow, with_owner: bool) -> Vec<u8> {
    let mut e = Vec::new();
    e.extend(fixed(&r.name, 10));
    e.extend(fixed(&r.lib, 10));
    e.extend(fixed(&r.ty, 10));
    if with_owner {
        e.extend(fixed(&r.owner, 10));
    }
    e.extend(fixed(if r.owned { "Y" } else { "N" }, 1));
    e.extend(fixed(auth_name(r.bits, r.ty == "*AUTL"), 10));
    e.extend(fixed(&auth_flags(r.bits), 11));
    e
}

fn list_answer(
    j: &mut Job,
    a: &Args<'_>,
    errc: ErrCode,
    format: &str,
    api: &str,
    input: &[u8],
    entries: &[Vec<u8>],
    size: usize,
) {
    match crate::obj::write_list(j, a, 0, format, api, input, &[], entries, size) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QSYLOBJA(space, format, user, type, returned, handle, errc)`:
/// `OBJA0100`, the objects a user owns or has a private authority to.
fn qsylobja(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 6);
    let format = a.char(1, 8);
    if !matches!(format.as_str(), "OBJA0100" | "OBJA0200") {
        errc.fail(j, &bad_format(&format));
        return;
    }
    let user = user_or_current(j, a.char(2, 10));
    let ty = a.char(3, 10);
    let which = a.char(4, 10);
    let Some(Answer::Objects { rows }) = ask(j, p, errc, Query::Objects { user, kind: 0 }) else {
        return;
    };
    let entries: Vec<Vec<u8>> = rows
        .iter()
        .filter(|r| ty.is_empty() || ty == "*ALL" || r.ty == ty)
        .filter(|r| match which.as_str() {
            "*OBJOWN" => r.owned,
            "*OBJAUT" => !r.owned,
            _ => true,
        })
        .map(|r| obj_entry(r, true))
        .collect();
    let input = [
        a.bytes(0, 20),
        fixed(&format, 8),
        a.bytes(2, 10),
        a.bytes(3, 10),
        a.bytes(4, 10),
    ]
    .concat();
    list_answer(j, &a, errc, &format, "QSYLOBJA", &input, &entries, 62);
}

/// `QSYLOBJP(space, format, user, type, errc)`: `OBJP0100`, the
/// objects whose primary group the user is.
fn qsylobjp(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(1, 8);
    if format != "OBJP0100" {
        errc.fail(j, &bad_format(&format));
        return;
    }
    let user = user_or_current(j, a.char(2, 10));
    let ty = a.char(3, 10);
    let Some(Answer::Objects { rows }) = ask(j, p, errc, Query::Objects { user, kind: 1 }) else {
        return;
    };
    let entries: Vec<Vec<u8>> = rows
        .iter()
        .filter(|r| ty.is_empty() || ty == "*ALL" || r.ty == ty)
        .map(|r| obj_entry(r, true))
        .collect();
    let input = [
        a.bytes(0, 20),
        fixed(&format, 8),
        a.bytes(2, 10),
        a.bytes(3, 10),
    ]
    .concat();
    list_answer(j, &a, errc, &format, "QSYLOBJP", &input, &entries, 62);
}

fn user_entry(r: &UserRow, autl: bool) -> Vec<u8> {
    let mut e = Vec::new();
    e.extend(fixed(&r.name, 10));
    e.extend(fixed(if r.group { "Y" } else { "N" }, 1));
    e.extend(fixed(auth_name(r.bits, autl), 10));
    e.extend(fixed(if r.bits & 0x400 != 0 { "Y" } else { "N" }, 1));
    e.extend(fixed(&auth_flags(r.bits), 11));
    e.extend(fixed("", 7));
    e
}

/// `QSYRAUTU(receiver, len, format, feedback, object, type, errc)`:
/// `RTUA0100`, one entry per profile holding authority, `*PUBLIC` last.
/// The feedback is bytes returned and available, the entries and their
/// length.
fn qsyrautu(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 6);
    let format = a.char(2, 8);
    if format != "RTUA0100" {
        errc.fail(j, &bad_format(&format));
        return;
    }
    let (name, lib) = a.qual(4);
    let ty = a.char(5, 10);
    let autl = ty == "*AUTL";
    let Some(Answer::Users { rows }) = ask(
        j,
        p,
        errc,
        Query::Holders {
            object: (name, lib, ty),
        },
    ) else {
        return;
    };
    let entries: Vec<u8> = rows.iter().flat_map(|r| user_entry(r, autl)).collect();
    let room = usize::try_from(a.bin4(1)).unwrap_or(0);
    let n = room.min(entries.len());
    if n > 0 {
        a.put_bytes(0, &entries[..n]);
    }
    let size = 40;
    let mut fb = Out::new();
    fb.bin4(0, 16);
    fb.bin4(4, 16);
    fb.bin4(8, i32::try_from(rows.len()).unwrap_or(0));
    fb.bin4(12, size);
    fb.deliver_raw(&a, 3, 16);
    errc.ok();
}

/// `QSYLAUTU(space, format, selection, group, errc)`: `AUTU0100`,
/// the profiles: `*ALL`, `*USRPRF`, `*GROUP` or `*MEMBER` of a group.
fn qsylautu(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(1, 8);
    if format != "AUTU0100" {
        errc.fail(j, &bad_format(&format));
        return;
    }
    let select = a.char(2, 10);
    let group = a.char(3, 10);
    let Some(Answer::Users { rows }) = ask(j, p, errc, Query::Users { select, group }) else {
        return;
    };
    let entries: Vec<Vec<u8>> = rows
        .iter()
        .map(|r| {
            let mut e = Vec::new();
            e.extend(fixed(&r.name, 10));
            e.extend(fixed(if r.group { "Y" } else { "N" }, 1));
            e.extend(fixed(if r.members { "Y" } else { "N" }, 1));
            e.extend(fixed(&r.text, 50));
            e
        })
        .collect();
    let input = [
        a.bytes(0, 20),
        fixed(&format, 8),
        a.bytes(2, 10),
        a.bytes(3, 10),
    ]
    .concat();
    list_answer(j, &a, errc, &format, "QSYLAUTU", &input, &entries, 62);
}

/// `QSYLATLO(space, format, list, errc)`: `ATLO0100`, the objects an
/// authorization list secures.
fn qsylatlo(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let format = a.char(1, 8);
    if format != "ATLO0100" {
        errc.fail(j, &bad_format(&format));
        return;
    }
    let list = a.char(2, 10);
    let Some(Answer::Objects { rows }) = ask(j, p, errc, Query::ListObjects { list }) else {
        return;
    };
    let entries: Vec<Vec<u8>> = rows
        .iter()
        .map(|r| {
            let mut e = Vec::new();
            e.extend(fixed(&r.name, 10));
            e.extend(fixed(&r.lib, 10));
            e.extend(fixed(&r.ty, 10));
            e.extend(fixed(&r.owner, 10));
            e
        })
        .collect();
    let input = [a.bytes(0, 20), fixed(&format, 8), a.bytes(2, 10)].concat();
    list_answer(j, &a, errc, &format, "QSYLATLO", &input, &entries, 40);
}

/// A password parameter: `CHAR(10)`, or `len` characters when the
/// optional length parameter at `len_at` is passed.
fn password(a: &Args<'_>, i: usize, len_at: usize) -> String {
    if a.present(len_at) {
        let n = usize::try_from(a.bin4(len_at)).unwrap_or(0).min(512);
        ccsid::from_ebcdic(&a.bytes(i, n))
    } else {
        a.char(i, 10)
    }
}

/// `QSYGETPH(user, password, handle, errc [, length, CCSID])`.
fn qsygetph(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let user = a.char(0, 10);
    let pw = password(&a, 1, 4);
    if let Some(Answer::Bytes { data }) = ask(j, p, errc, Query::GetHandle { user, password: pw }) {
        let mut h = data;
        h.resize(12, 0);
        a.put_bytes(2, &h);
        errc.ok();
    }
}

/// `QWTSETP(handle, errc)`: the job runs as the handle's user.
fn qwtsetp(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 1);
    let handle = a.bytes(0, 12);
    if ask(j, p, errc, Query::SetHandle { handle }).is_some() {
        errc.ok();
    }
}

/// `QSYRLSPH(handle, errc)`.
fn qsyrlsph(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 1);
    let handle = a.bytes(0, 12);
    if ask(j, p, errc, Query::ReleaseHandle { handle }).is_some() {
        errc.ok();
    }
}

/// `QSYCHGPW(user, current, new, errc [, len, CCSID, len, CCSID])`:
/// the validation program runs here, in the job, first.
fn qsychgpw(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 3);
    let user = user_or_current(j, a.char(0, 10));
    let current = password(&a, 1, 4);
    let new = password(&a, 2, 6);
    match validation(j, p, &user, &new, &current) {
        Ok(true) => {}
        Ok(false) => {
            errc.fail(j, &ApiFail::id("CPF22C6"));
            return;
        }
        Err(f) => {
            errc.fail(j, &f);
            return;
        }
    }
    let q = Query::ChangePassword { user, current, new };
    if ask(j, p, errc, q).is_some() {
        errc.ok();
    }
}

/// `QPWDVLDPGM`, when it names a program: `Ok(false)` when it refuses.
fn validation(
    j: &mut Job,
    p: &mut dyn Provider,
    user: &str,
    new: &str,
    old: &str,
) -> Result<bool, ApiFail> {
    let Some(v) = p.sysval("QPWDVLDPGM") else {
        return Ok(true);
    };
    let text = ccsid::from_ebcdic(&v.data);
    let mut w = text.split_whitespace();
    let Some(name) = w.next().map(str::to_ascii_uppercase) else {
        return Ok(true);
    };
    if name.starts_with('*') {
        return Ok(true);
    }
    let lib = w
        .next()
        .map_or_else(|| "*LIBL".to_string(), str::to_ascii_uppercase);
    let level = p
        .sysval("QPWDLVL")
        .and_then(|v| {
            v.data
                .get(..4)
                .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        })
        .unwrap_or(3);
    let parm = |s: &str| -> Vec<u8> {
        let t = ccsid::to_ebcdic(s).unwrap_or_default();
        if level <= 1 {
            fixed(s, 10)
        } else {
            let mut b = u32::try_from(t.len()).unwrap_or(0).to_be_bytes().to_vec();
            b.extend_from_slice(&37u32.to_be_bytes());
            b.extend(t);
            b
        }
    };
    let mut new_b = parm(new);
    let mut old_b = parm(old);
    let mut rc = vec![0xF0u8];
    let mut usr = fixed(user, 10);
    let addrs = [
        new_b.as_mut_ptr() as u64,
        old_b.as_mut_ptr() as u64,
        rc.as_mut_ptr() as u64,
        usr.as_mut_ptr() as u64,
    ];
    j.call_program(&name, &lib, &addrs)
        .map_err(|e| ApiFail::id(&e.msgid()))?;
    Ok(rc[0] == 0xF0)
}

/// `QSYRUPWD(receiver, len, format, user, errc)`: `UPWD0100`, the
/// profile name and QS/400's password data (not IBM's format).
fn qsyrupwd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(2, 8);
    if format != "UPWD0100" {
        errc.fail(j, &bad_format(&format));
        return;
    }
    let user = a.char(3, 10);
    let Some(Answer::Bytes { data }) = ask(j, p, errc, Query::GetPassword { user: user.clone() })
    else {
        return;
    };
    let mut out = Out::new();
    out.char(8, 10, &user);
    out.bytes(18, &data);
    out.deliver(&a, 0, 1);
    errc.ok();
}

/// `QSYSUPWD(data, format, errc)`: `UPWD0100` as `QSYRUPWD` gave it.
fn qsysupwd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 2);
    let format = a.char(1, 8);
    if format != "UPWD0100" {
        errc.fail(j, &bad_format(&format));
        return;
    }
    let head = a.bytes(0, 18);
    let len = head.get(..4).map_or(0, |b| {
        usize::try_from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])).unwrap_or(0)
    });
    let user = ccsid::decode_trimmed(head.get(8..18).unwrap_or(&[]));
    let all = a.bytes(0, len.max(18));
    let data = all.get(18..).unwrap_or(&[]).to_vec();
    if ask(j, p, errc, Query::SetPassword { user, data }).is_some() {
        errc.ok();
    }
}
