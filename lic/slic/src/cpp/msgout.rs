// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages a command sends as their catalogue entries: the identifier
//! IBM gives the meaning, the data laid out as the entry declares it
//! (IBM's field formats, `tools/conformance/oracle/msgdata/fields.txt`)
//! and no words of the command's own, so Command Entry and the job log
//! show the catalogue's text and a program that monitors for the
//! message reads the data it expects.

use stg::proto::{CmdMsg, CmdReply};

/// The job log's message types.
const DIAGNOSTIC: u8 = 2;
const COMPLETION: u8 = 4;
const ESCAPE: u8 = 7;

/// A detail the catalogue's message cannot hold: `CPF9897`, `&1`.
const DETAIL: &str = "CPF9897";

/// One field of a message's data.
#[derive(Debug, Clone, Copy)]
pub enum Field<'a> {
    /// `*CHAR n`: CP037, padded with blanks or cut to `n`.
    Char(&'a str, usize),
    /// `*BIN 2`.
    Bin2(i16),
    /// `*BIN 4`.
    Bin4(i32),
    /// `*DTS`: microseconds since the Unix epoch, as `msgf` reads it.
    Dts(i64),
}

/// The data of `fields`, one after another.
#[must_use]
pub fn data(fields: &[Field<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in fields {
        match *f {
            Field::Char(text, n) => out.extend_from_slice(&cmd::error::data(&[(text, n)])),
            Field::Bin2(v) => out.extend_from_slice(&v.to_be_bytes()),
            Field::Bin4(v) => out.extend_from_slice(&v.to_be_bytes()),
            Field::Dts(v) => out.extend_from_slice(&v.to_be_bytes()),
        }
    }
    out
}

/// A message of `kind` with its data.
#[must_use]
pub fn msg(msgid: &str, kind: u8, fields: &[Field<'_>]) -> CmdMsg {
    CmdMsg {
        msgid: msgid.into(),
        kind,
        data: data(fields),
    }
}

/// The command ended with escape `msgid`. A `detail` the entry cannot
/// hold (the cause underneath, as a device or the store gave it) goes
/// before it as the diagnostic `CPF9897`.
#[must_use]
pub fn escape(msgid: &str, fields: &[Field<'_>], detail: &str) -> CmdReply {
    let mut messages = Vec::new();
    if !detail.trim().is_empty() {
        messages.push(msg(DETAIL, DIAGNOSTIC, &[Field::Char(detail.trim(), 512)]));
    }
    CmdReply {
        messages,
        escape: Some(msg(msgid, ESCAPE, fields)),
        ..CmdReply::default()
    }
}

/// The command ended with completion message `msgid`.
#[must_use]
pub fn completion(msgid: &str, fields: &[Field<'_>]) -> CmdReply {
    CmdReply {
        messages: vec![msg(msgid, COMPLETION, fields)],
        ..CmdReply::default()
    }
}

/// A diagnostic for the job log before the command's last message.
#[must_use]
pub fn diagnostic(msgid: &str, fields: &[Field<'_>]) -> CmdMsg {
    msg(msgid, DIAGNOSTIC, fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_laid_out_as_declared() {
        let d = data(&[
            Field::Bin4(3),
            Field::Bin4(0),
            Field::Char("QSTEST", 10),
            Field::Bin2(1),
        ]);
        assert_eq!(d.len(), 4 + 4 + 10 + 2);
        assert_eq!(&d[..4], &[0, 0, 0, 3]);
        assert_eq!(d[8], 0xd8, "Q in CP037");
        assert_eq!(&d[14..18], &[0x40; 4]);
        assert_eq!(&d[18..20], &[0, 1]);
    }

    #[test]
    fn an_escape_has_no_text_of_its_own() {
        let r = escape("CPF6768", &[Field::Char("TAP01", 10)], "");
        assert!(r.text.is_empty());
        assert!(r.messages.is_empty());
        let e = r.escape.expect("escape");
        assert_eq!((e.msgid.as_str(), e.kind, e.data.len()), ("CPF6768", 7, 10));
        let r = escape("CPF6708", &[], "I/O error (os error 5)");
        assert_eq!(r.messages[0].msgid, "CPF9897");
        assert_eq!(r.messages[0].kind, 2);
    }
}
