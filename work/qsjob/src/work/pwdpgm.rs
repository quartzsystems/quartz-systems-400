// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QPWDVLDPGM`: the program that may refuse a new password, called in
//! the job through the ILE call path with IBM's parameters: the new
//! password, the old one, a one-character return code (`0` accepts)
//! and the profile name. At password levels 0 and 1 the passwords are
//! `CHAR(10)`; at 2 and 3 each is a four-byte length, a four-byte CCSID
//! (37) and the characters.

use stg::{Client, Shared};

use crate::console::Console;

/// The program `QPWDVLDPGM` names, `(lib, name)`; `None` for `*NONE`
/// and `*REGFAC`.
pub fn program(link: &Shared) -> Option<(String, String)> {
    let v = link.borrow_mut().with(|l| l.sysval("QPWDVLDPGM".into()))?;
    let text = ccsid::from_ebcdic(&v.data);
    let mut words = text.split_whitespace();
    let name = words.next()?.to_ascii_uppercase();
    if name.starts_with('*') {
        return None;
    }
    let lib = words
        .next()
        .map_or_else(|| "*LIBL".to_string(), str::to_ascii_uppercase);
    Some((lib, name))
}

fn level(link: &Shared) -> u8 {
    link.borrow_mut()
        .with(|l| l.sysval("QPWDLVL".into()))
        .and_then(|v| {
            v.data
                .get(..4)
                .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        })
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(3)
}

fn password_parm(p: &str, level: u8) -> Vec<u8> {
    let text = ccsid::to_ebcdic(p).unwrap_or_default();
    if level <= 1 {
        let mut b = text;
        b.resize(10, 0x40);
        b.truncate(10);
        return b;
    }
    let mut b = Vec::with_capacity(8 + text.len());
    b.extend_from_slice(&u32::try_from(text.len()).unwrap_or(0).to_be_bytes());
    b.extend_from_slice(&37u32.to_be_bytes());
    b.extend_from_slice(&text);
    b
}

/// Run the validation program, if one is named: `Ok(true)` when it
/// accepted or there is none, `Ok(false)` when it refused.
pub fn validate(
    c: &mut Console,
    link: &Shared,
    user: &str,
    new: &str,
    old: &str,
) -> Result<bool, String> {
    let Some((lib, name)) = program(link) else {
        return Ok(true);
    };
    let lvl = level(link);
    let mut new_b = password_parm(new, lvl);
    let mut old_b = password_parm(old, lvl);
    let mut rc = vec![0xF0u8];
    let mut profile = ccsid::to_ebcdic(user).unwrap_or_default();
    profile.resize(10, 0x40);
    let addrs = [
        new_b.as_mut_ptr() as u64,
        old_b.as_mut_ptr() as u64,
        rc.as_mut_ptr() as u64,
        profile.as_mut_ptr() as u64,
    ];
    let j = c.job();
    j.call_program(&name, &lib, &addrs).map_err(|e| {
        format!(
            "{}: the password validation program {lib}/{name} could not be called",
            e.msgid()
        )
    })?;
    if let Some(e) = j.clear() {
        return Err(format!("{} in the password validation program", e.msgid));
    }
    Ok(rc[0] == 0xF0)
}
