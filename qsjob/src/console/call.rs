// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `call LIB/PGM [parms]`: parameters as CL types them, the program's
//! console lines and changed parameters afterwards.

use std::fmt::Write as _;

use super::Console;

/// The parameter values of a `call` line: `'text'` as characters,
/// numbers as `packed(15,5)`, `x'..'` as bytes.
fn call_parms(rest: &str) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        if chars[i] == '\''
            || (chars[i].eq_ignore_ascii_case(&'x') && chars.get(i + 1) == Some(&'\''))
        {
            let hex = chars[i] != '\'';
            i += if hex { 2 } else { 1 };
            let start = i;
            while i < chars.len() && chars[i] != '\'' {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            i += 1;
            if hex {
                let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                let bytes = (0..clean.len() / 2)
                    .map(|k| {
                        u8::from_str_radix(&clean[2 * k..2 * k + 2], 16).map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<u8>, String>>()?;
                out.push(bytes);
            } else {
                out.push(ccsid::to_ebcdic(&text).map_err(|_| format!("{text}: not CP037"))?);
            }
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let d = qdecimal::Decimal::parse(&word).map_err(|_| format!("{word}: not a number"))?;
        let p = qdecimal::Packed::new(15, 5).expect("valid");
        out.push(
            p.encode(&d, qdecimal::Rounding::Truncate)
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(out)
}

/// `call LIB/PGM [parms]`.
pub fn call(c: &mut Console, line: &str) -> Result<String, String> {
    let rest = line.trim_start()[4..].trim_start();
    let (qual, parms) = rest
        .find(char::is_whitespace)
        .map_or((rest, ""), |i| (&rest[..i], &rest[i..]));
    if qual.is_empty() {
        return Err("call LIB/PGM [parms]".into());
    }
    let (lib, name) = qual
        .split_once('/')
        .map_or(("*LIBL", qual), |(l, n)| (l, n));
    let (lib, name) = (lib.to_ascii_uppercase(), name.to_ascii_uppercase());
    let mut buffers = call_parms(parms)?;
    let addrs: Vec<u64> = buffers.iter_mut().map(|b| b.as_mut_ptr() as u64).collect();
    let j = c.job();
    let before = j.console.len();
    let r = j.call_program(&name, &lib, &addrs);
    let lines: Vec<String> = j.console.drain(before..).collect();
    let pending = j.clear();
    let mut out = String::new();
    for l in lines {
        let _ = writeln!(out, "{l}");
    }
    if let Err(e) = r {
        let _ = writeln!(out, "{}: {lib}/{name} could not be called.", e.msgid());
    }
    if let Some(e) = pending {
        if e.msgid == "CEE9903" {
            let _ = writeln!(out, "Program ended by CEETREC.");
        } else {
            let _ = writeln!(out, "Exception {} left for the caller.", e.msgid);
        }
    }
    for (i, b) in buffers.iter().enumerate() {
        if b.iter().all(u8::is_ascii_graphic) {
            continue;
        }
        let _ = writeln!(out, "Parm {}: {}", i + 1, parm_text(b));
    }
    out.push_str(&c.pending());
    Ok(out)
}

/// A parameter after the call: its text when it is text, else hex.
fn parm_text(b: &[u8]) -> String {
    let text = ccsid::from_ebcdic(b);
    if text.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        format!("'{}'", text.trim_end())
    } else {
        b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02X}");
            s
        })
    }
}
