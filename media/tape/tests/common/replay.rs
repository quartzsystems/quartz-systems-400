// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Replay of a hand-written ioctl transcript (`tests/fixtures/*.txt`).
//!
//! One call per line, `CALL ARGS -> RESULT`; `#` starts a comment. The
//! calls and results:
//!
//! ```text
//! mtop MTREW 1 -> ok | EIO | ENOSPC | ...
//! mtget -> gstat=BOT,ONLINE dsreg=0x58000000 fileno=0 blkno=0 | EIO
//! mtpos -> 3
//! read LEN -> N [hex=..] [pad=XX] | ERRNO     (N bytes: hex, then pad)
//! write LEN -> N | ERRNO
//! sg_io CDB-HEX DIR LEN -> status=S [resid=R] [host=H] [sense=HEX]
//!   data ...                                   (the data-in bytes)
//! ```
//!
//! A `data` line holds hex bytes, `"text"` (ASCII), `spaces:N` and
//! `zeros:N`. Each call must match the next line, or the test fails.

use std::collections::VecDeque;
use std::io;

use tape::st::ioctl::{
    GMT_BOT, GMT_DR_OPEN, GMT_EOD, GMT_EOF, GMT_EOT, GMT_ONLINE, GMT_WR_PROT, MtGet, MtOp, SgDir,
    SgIoctl, SgReply, StIoctl,
};

#[derive(Debug)]
struct Step {
    line: usize,
    call: String,
    result: String,
    data: Vec<u8>,
}

/// A transcript being replayed.
#[derive(Debug)]
pub struct Replay {
    steps: VecDeque<Step>,
}

fn errno(name: &str) -> Option<i32> {
    Some(match name {
        "EIO" => 5,
        "ENOMEM" => 12,
        "EACCES" => 13,
        "EINVAL" => 22,
        "ENOSPC" => 28,
        "EROFS" => 30,
        "ENOMEDIUM" => 123,
        _ => return None,
    })
}

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(s.len().is_multiple_of(2), "odd hex {s}");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn data_line(s: &str, out: &mut Vec<u8>) {
    let mut rest = s.trim();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('"') {
            let (text, tail) = r.split_once('"').expect("closing quote");
            out.extend_from_slice(text.as_bytes());
            rest = tail.trim_start();
            continue;
        }
        let (tok, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        rest = tail.trim_start();
        if let Some(n) = tok.strip_prefix("spaces:") {
            out.resize(out.len() + n.parse::<usize>().unwrap(), b' ');
        } else if let Some(n) = tok.strip_prefix("zeros:") {
            out.resize(out.len() + n.parse::<usize>().unwrap(), 0);
        } else {
            out.extend(hex(tok));
        }
    }
}

fn kv<'a>(result: &'a str, key: &str) -> Option<&'a str> {
    result
        .split_whitespace()
        .find_map(|t| t.strip_prefix(key)?.strip_prefix('='))
}

fn num(s: &str) -> i64 {
    s.strip_prefix("0x")
        .map_or_else(|| s.parse().unwrap(), |h| i64::from_str_radix(h, 16).unwrap())
}

impl Replay {
    /// Parse a transcript.
    pub fn parse(text: &str) -> Self {
        let mut steps: VecDeque<Step> = VecDeque::new();
        for (i, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("");
            if line.trim().is_empty() {
                continue;
            }
            if let Some(d) = line.trim_start().strip_prefix("data ") {
                data_line(d, &mut steps.back_mut().expect("data after a call").data);
                continue;
            }
            let (call, result) = line.split_once("->").expect("CALL -> RESULT");
            steps.push_back(Step {
                line: i + 1,
                call: call.split_whitespace().collect::<Vec<_>>().join(" "),
                result: result.trim().to_string(),
                data: Vec::new(),
            });
        }
        Self { steps }
    }

    fn next(&mut self, call: &str) -> Step {
        let step = self
            .steps
            .pop_front()
            .unwrap_or_else(|| panic!("transcript ended; call was {call:?}"));
        assert_eq!(step.call, call, "transcript line {}", step.line);
        step
    }

    /// Every step was used.
    pub fn finish(&self) {
        assert!(self.steps.is_empty(), "unused steps: {:?}", self.steps);
    }

    fn err(result: &str) -> Option<io::Error> {
        errno(result.split_whitespace().next()?).map(io::Error::from_raw_os_error)
    }
}

fn gstat(names: &str) -> u64 {
    names
        .split(',')
        .map(|n| match n {
            "BOT" => GMT_BOT,
            "EOF" => GMT_EOF,
            "EOT" => GMT_EOT,
            "EOD" => GMT_EOD,
            "WR_PROT" => GMT_WR_PROT,
            "ONLINE" => GMT_ONLINE,
            "DR_OPEN" => GMT_DR_OPEN,
            "" => 0,
            other => panic!("unknown gstat bit {other}"),
        })
        .fold(0, |a, b| a | b)
}

impl StIoctl for Replay {
    fn mtop(&mut self, op: MtOp, count: i32) -> io::Result<()> {
        let s = self.next(&format!("mtop {} {count}", op.name()));
        if let Some(e) = Self::err(&s.result) { Err(e) } else {
            assert_eq!(s.result, "ok", "line {}", s.line);
            Ok(())
        }
    }

    fn mtget(&mut self) -> io::Result<MtGet> {
        let s = self.next("mtget");
        if let Some(e) = Self::err(&s.result) {
            return Err(e);
        }
        Ok(MtGet {
            mt_type: kv(&s.result, "type").map_or(0, num),
            resid: 0,
            dsreg: kv(&s.result, "dsreg").map_or(0, num).cast_unsigned(),
            gstat: gstat(kv(&s.result, "gstat").unwrap_or("")),
            erreg: 0,
            fileno: kv(&s.result, "fileno").map_or(0, |v| i32::try_from(num(v)).unwrap()),
            blkno: kv(&s.result, "blkno").map_or(0, |v| i32::try_from(num(v)).unwrap()),
        })
    }

    fn mtpos(&mut self) -> io::Result<u64> {
        let s = self.next("mtpos");
        Self::err(&s.result).map_or_else(|| Ok(num(&s.result).cast_unsigned()), Err)
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let s = self.next(&format!("read {}", buf.len()));
        if let Some(e) = Self::err(&s.result) {
            return Err(e);
        }
        let n = usize::try_from(num(s.result.split_whitespace().next().unwrap())).unwrap();
        let head = kv(&s.result, "hex").map(hex).unwrap_or_default();
        let pad = kv(&s.result, "pad").map_or(0, |p| hex(p)[0]);
        buf[..n].fill(pad);
        buf[..head.len()].copy_from_slice(&head);
        Ok(n)
    }

    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let s = self.next(&format!("write {}", data.len()));
        Self::err(&s.result).map_or_else(
            || Ok(usize::try_from(num(&s.result)).unwrap()),
            Err,
        )
    }
}

impl SgIoctl for Replay {
    fn sg_io(
        &mut self,
        cdb: &[u8],
        dir: SgDir,
        data: &mut [u8],
        _timeout_ms: u32,
    ) -> io::Result<SgReply> {
        let cdb_hex: Vec<String> = cdb.iter().map(|b| format!("{b:02x}")).collect();
        let dir = match dir {
            SgDir::None => "none",
            SgDir::ToDevice => "to",
            SgDir::FromDevice => "from",
        };
        let s = self.next(&format!("sg_io {} {dir} {}", cdb_hex.join(" "), data.len()));
        if let Some(e) = Self::err(&s.result) {
            return Err(e);
        }
        data[..s.data.len()].copy_from_slice(&s.data);
        let n = |k| kv(&s.result, k).map_or(0, num);
        Ok(SgReply {
            status: u8::try_from(n("status")).unwrap(),
            host_status: u16::try_from(n("host")).unwrap(),
            driver_status: u16::try_from(n("driver")).unwrap(),
            sense: kv(&s.result, "sense").map(hex).unwrap_or_default(),
            resid: i32::try_from(n("resid")).unwrap(),
        })
    }
}
