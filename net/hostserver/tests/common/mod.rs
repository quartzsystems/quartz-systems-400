// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Replaying committed traces: the client's frames go to our server in
//! order, and each of our replies is compared with the oracle's,
//! field by field, after masking what differs by nature (job numbers,
//! times, file IDs). Each difference is a line; the lines a scenario is
//! known to differ by are kept in `tests/replay/<server>-<scenario>.txt`
//! (`QS400_UPDATE_REPLAY=1` rewrites them), so a change in our replies
//! shows as a failing test.

#![allow(dead_code)]

pub mod mem;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use hostserver::{Auth, Handler, Proof};
use hostsvr::frame::Frame;
use hostsvr::trace::{Dir, Event, Trace};

/// The committed traces.
pub fn traces_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/hostsvr/traces")
}

/// One trace.
pub fn trace(server: &str, scenario: &str) -> Trace {
    let p = traces_dir().join(server).join(format!("{scenario}.trace"));
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    Trace::parse(&text).unwrap()
}

/// Signing on as the trace did: the oracle's seeds and job name, every
/// proof accepted (the trace's substitutes are masked).
#[derive(Clone, Debug, Default)]
pub struct TraceAuth {
    /// Server seeds, in order (shared by every connection's server).
    pub seeds: std::rc::Rc<std::cell::RefCell<VecDeque<[u8; 8]>>>,
    /// The job name the oracle answered.
    pub job: String,
}

impl TraceAuth {
    /// From a trace's replies.
    pub fn of(t: &Trace) -> Self {
        let mut a = Self::default();
        for c in &t.connections {
            for ch in c.chunks() {
                if ch.dir != Dir::Reply || !ch.framed {
                    continue;
                }
                let Ok(f) = Frame::decode(&ch.bytes) else { continue };
                match f.header.req_rep_id {
                    0xF001 => {
                        let mut s = [0u8; 8];
                        s.copy_from_slice(&f.template[4..12]);
                        a.seeds.borrow_mut().push_back(s);
                    }
                    0xF002 => {
                        if let Some(p) = f.param(0x111F) {
                            a.job = hostserver::text::from_ebcdic(&p.data[4..]);
                        }
                    }
                    _ => {}
                }
            }
        }
        if a.job.is_empty() {
            a.job = "000000/QUSER/QZTEST".into();
        }
        a
    }
}

impl Auth for TraceAuth {
    fn password_level(&mut self) -> u8 {
        0
    }
    fn seed(&mut self) -> [u8; 8] {
        self.seeds.borrow_mut().pop_front().unwrap_or([7; 8])
    }
    fn start(&mut self, _user: &str, _proof: Proof<'_>) -> Result<(), u32> {
        Ok(())
    }
    fn job_name(&mut self) -> String {
        self.job.clone()
    }
}

/// Byte ranges (template offsets, or per code point data offsets) that
/// differ by nature and are not compared.
#[derive(Clone, Debug, Default)]
pub struct Masks {
    /// Template byte ranges, by reply ID.
    pub template: Vec<(u16, std::ops::Range<usize>)>,
    /// Parameter data ranges, by code point.
    pub param: Vec<(u16, std::ops::Range<usize>)>,
    /// Code points whose data is not compared at all.
    pub skip: Vec<u16>,
}

fn masked(b: &[u8], ranges: impl Iterator<Item = std::ops::Range<usize>>) -> Vec<u8> {
    let mut v = b.to_vec();
    for r in ranges {
        for i in r {
            if let Some(x) = v.get_mut(i) {
                *x = 0;
            }
        }
    }
    v
}

fn ranges_of(a: &[u8], b: &[u8]) -> String {
    let n = a.len().max(b.len());
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..=n {
        let differ = i < n && a.get(i) != b.get(i);
        match (differ, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push(if i - s == 1 { format!("{s}") } else { format!("{s}-{}", i - 1) });
                start = None;
            }
            _ => {}
        }
    }
    let mut s = out.join(",");
    if s.len() > 60 {
        s.truncate(60);
        s.push_str("...");
    }
    s
}

/// Compare one of our replies with the oracle's.
pub fn compare(at: &str, ours: &[u8], theirs: &[u8], m: &Masks, out: &mut Vec<String>) {
    let (Ok(a), Ok(b)) = (Frame::decode(ours), Frame::decode(theirs)) else {
        if ours != theirs {
            out.push(format!("{at}: raw bytes differ"));
        }
        return;
    };
    let (mut a, mut b) = (a, b);
    // Compressed database parameters compare as what they expand to.
    for f in [&mut a, &mut b] {
        if f.param(0x3832).is_some()
            && let Ok(p) = hostsvr::database::param::expanded(&f.params)
        {
            f.params = p;
        }
    }
    let id = b.header.req_rep_id;
    if a.header.req_rep_id != id {
        out.push(format!("{at}: reply {:04x}, oracle {:04x}", a.header.req_rep_id, id));
        return;
    }
    if a.header.header_id != b.header.header_id || a.header.correlation != b.header.correlation || a.header.server_id != b.header.server_id {
        out.push(format!("{at}: header fields differ"));
    }
    let tr = || m.template.iter().filter(move |(i, _)| *i == id).map(|(_, r)| r.clone());
    let (ta, tb) = (masked(&a.template, tr()), masked(&b.template, tr()));
    if ta != tb {
        out.push(format!("{at}: template {} bytes, oracle {}, differ at {}", ta.len(), tb.len(), ranges_of(&ta, &tb)));
    }
    let cps = |f: &Frame| f.params.iter().map(|p| format!("{:04x}", p.cp)).collect::<Vec<_>>().join(" ");
    if cps(&a) != cps(&b) {
        out.push(format!("{at}: parameters [{}], oracle [{}]", cps(&a), cps(&b)));
    }
    for (pa, pb) in a.params.iter().zip(&b.params) {
        if pa.cp != pb.cp || m.skip.contains(&pa.cp) {
            continue;
        }
        let pr = || m.param.iter().filter(|(c, _)| *c == pa.cp).map(|(_, r)| r.clone());
        let (da, db) = (masked(&pa.data, pr()), masked(&pb.data, pr()));
        if da != db {
            out.push(format!(
                "{at}: {:04x} {} bytes, oracle {}, differ at {}",
                pa.cp,
                da.len(),
                db.len(),
                ranges_of(&da, &db)
            ));
        }
    }
}

/// Replay a framed trace against servers made by `make` (one per
/// connection); the difference lines.
#[allow(clippy::many_single_char_names)]
pub fn replay(t: &Trace, masks: &Masks, make: &mut dyn FnMut() -> Box<dyn Handler>) -> Vec<String> {
    let mut out = Vec::new();
    let mut compared = 0;
    for c in &t.connections {
        let mut h = make();
        let chunks: Vec<_> = c.chunks().collect();
        let mut i = 0;
        let mut closed = false;
        let mut n = 0;
        while i < chunks.len() {
            let ch = chunks[i];
            i += 1;
            if ch.dir != Dir::Request || !ch.framed {
                continue;
            }
            n += 1;
            let f = Frame::decode(&ch.bytes).unwrap();
            let at = format!(
                "conn {} req {n} {:04x}/{:04x} corr {}",
                c.id, f.header.server_id, f.header.req_rep_id, f.header.correlation
            );
            let mut theirs = Vec::new();
            while i < chunks.len() && chunks[i].dir == Dir::Reply {
                theirs.push(chunks[i].bytes.clone());
                i += 1;
            }
            if closed {
                out.push(format!("{at}: we had closed"));
                continue;
            }
            let o = h.handle(&ch.bytes);
            let oracle_closed = theirs.is_empty()
                && i >= chunks.len()
                && c.events.iter().any(|e| matches!(e, Event::Chunk(_)))
                && !o.replies.is_empty();
            if oracle_closed {
                out.push(format!("{at}: the oracle closed without a reply; we answer {}", o.replies.len()));
            } else if o.replies.len() != theirs.len() {
                out.push(format!("{at}: {} replies, oracle {}", o.replies.len(), theirs.len()));
            }
            for (k, (a, b)) in o.replies.iter().zip(&theirs).enumerate() {
                compared += 1;
                compare(&format!("{at} reply {}", k + 1), a, b, masks, &mut out);
            }
            closed = o.close;
        }
    }
    out.push(format!("{compared} replies compared"));
    out
}

/// Check the difference lines against the accepted ones.
pub fn accept(name: &str, diffs: &[String]) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replay").join(format!("{name}.txt"));
    let text = diffs.join("\n") + if diffs.is_empty() { "" } else { "\n" };
    if std::env::var_os("QS400_UPDATE_REPLAY").is_some() {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&p).unwrap_or_default().replace("\r\n", "\n");
    assert_eq!(text, want, "replay differences of {name} changed (QS400_UPDATE_REPLAY=1 rewrites {})", p.display());
}
