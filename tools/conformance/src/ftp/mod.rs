// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The FTP transcript replay (Phase 10 section 5): every session the
//! oracle recorded (`tools/conformance/ftp/transcripts/`) runs again
//! against our server in memory, from the world the fixture describes,
//! and each command's answer is compared with the oracle's: reply
//! codes, the replies clients parse, and the data (bytes, text lines,
//! listing fields). Commands where this server deliberately answers
//! otherwise (RFC 3659's `SIZE`, `MDTM`, `MLST`, `MLSD`, `REST`; `FEAT`
//! and `OPTS` listing them) are counted apart, as deviations.

pub mod compare;
pub mod fixture;
pub mod transcript;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use ftp::net::mem::MemNet;
use ftp::session::{Config, Flow, Session};
use servers::attrs::ftp::FtpAttrs;

use transcript::{DataIn, Transcript, codes};

/// What one session came to.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// The session (`category/name`).
    pub name: String,
    /// Commands compared.
    pub compared: usize,
    /// Commands that matched.
    pub matched: usize,
    /// Commands left out as deliberate deviations.
    pub deviations: usize,
    /// What differed, a line per command.
    pub diffs: Vec<String>,
}

impl Outcome {
    /// Whether every compared command matched.
    pub fn passed(&self) -> bool {
        self.compared == self.matched
    }

    /// The category: the session's folder (`seed` for the first ones).
    pub fn category(&self) -> &str {
        self.name.split_once('/').map_or("seed", |(c, _)| c)
    }
}

/// The verbs this server answers otherwise on purpose (docs/FTP.md).
const DEVIATING: &[&str] = &["SIZE", "MDTM", "MLST", "MLSD", "REST", "FEAT", "OPTS"];

/// The payloads of the first round's uploads, which were not recorded.
fn legacy_payload(n: usize) -> Vec<u8> {
    match n {
        54 => b"Quartz FTP probe line 1\r\nsecond line, 2\r\n\r\nlast line\r\n".to_vec(),
        256 => (0..=255u8).collect(),
        10 => b"appended\r\n".to_vec(),
        8 => b"unique\r\n".to_vec(),
        _ => vec![b'x'; n],
    }
}

/// The server as the oracle was set up (pub400: paths, the home
/// directory, Unix listings, a long idle limit).
pub fn oracle_config() -> Config {
    let mut a = FtpAttrs::default();
    for (k, v) in [
        ("NAMEFMT", "*PATH"),
        ("CURDIR", "*HOMEDIR"),
        ("LISTFMT", "*UNIX"),
        ("INACTTIMO", "99999"),
    ] {
        let _ = a.set(k, v);
    }
    Config::new(a, "QS400")
}

/// Our text with the replay's profile masked as the transcripts mask
/// the account.
fn mask(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let up = s.to_ascii_uppercase();
    let mut i = 0;
    while i < s.len() {
        if up[i..].starts_with(fixture::USER) {
            out.push_str("{user}");
            i += fixture::USER.len();
        } else {
            let c = s[i..].chars().next().unwrap_or(' ');
            out.push(c);
            i += c.len_utf8();
        }
    }
    out
}

/// The last line of a reply set, the one clients parse.
fn last(lines: &[String]) -> String {
    lines.last().map(|l| l.get(4..).unwrap_or("").trim().to_string()).unwrap_or_default()
}

/// Whether `gaps` names this session's command: one the fixture cannot
/// set up (a listing of what other profiles keep on the oracle).
fn gap(gaps: &str, session: &str, cmd: &str) -> bool {
    gaps.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .any(|(s, c)| s == session && c == cmd)
}

/// Replay one transcript.
pub fn replay(t: &Transcript, fixture_text: &str, gaps: &str) -> Outcome {
    let host = fixture::world(fixture_text, &t.name);
    let mut s = Session::new(host, oracle_config());
    let mut net = MemNet::default();
    let mut out = Outcome {
        name: t.name.clone(),
        ..Outcome::default()
    };
    if s.greet(&mut net) == Flow::Close {
        out.compared = 1;
        out.diffs.push("the banner closed the session".into());
        return out;
    }
    net.take_replies();
    let mut rest_pending = false;
    let mut ebcdic = false;
    for (i, step) in t.steps.iter().enumerate() {
        if t.cut.is_some_and(|c| i >= c) {
            break;
        }
        let line = if step.cmd.starts_with("PASS ") {
            format!("PASS {}", fixture::PASSWORD)
        } else {
            step.cmd.replace("{user}", fixture::USER)
        };
        if let Some((n, p)) = &step.sent {
            net.uploads.push_back(p.clone().unwrap_or_else(|| legacy_payload(*n)));
        }
        let flow = s.handle(&line, &mut net);
        let replies = net.take_replies();
        let sent = net.take_sent();
        net.uploads.clear();
        let verb = step.cmd.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
        if verb == "TYPE" && replies.first().is_some_and(|r| r.code == 200) {
            let t = step.cmd.split_whitespace().nth(1).unwrap_or("").to_ascii_uppercase();
            ebcdic = t == "E" || t == "F";
        }
        if gap(gaps, &t.name, &step.cmd) {
            out.deviations += 1;
            continue;
        }
        let deviating = DEVIATING.contains(&verb.as_str()) || (rest_pending && matches!(verb.as_str(), "RETR" | "STOR"));
        rest_pending = verb == "REST" || (rest_pending && !matches!(verb.as_str(), "RETR" | "STOR" | "APPE"));
        if deviating {
            out.deviations += 1;
            if flow == Flow::Close {
                break;
            }
            continue;
        }
        out.compared += 1;
        let mut diffs = Vec::new();
        let ours_lines: Vec<String> = replies.iter().flat_map(|r| r.render().lines().map(str::to_string).collect::<Vec<_>>()).collect();
        let theirs_codes = codes(&step.replies);
        let ours_codes: Vec<u16> = replies.iter().map(|r| r.code).collect();
        if theirs_codes == ours_codes {
            let parsed = matches!(verb.as_str(), "PWD" | "XPWD" | "CWD" | "XCWD" | "CDUP" | "XCUP" | "SYST");
            if parsed && theirs_codes.last().is_some_and(|c| *c == 257 || *c == 250 || *c == 215) {
                let mut theirs = last(&step.replies);
                let mut ours = mask(&last(&ours_lines));
                if verb == "SYST" {
                    theirs = theirs.replace("V7R5M0", "VxRyMz");
                    ours = ours.replace(&release::RELEASE.to_string(), "VxRyMz");
                }
                if theirs != ours {
                    diffs.push(format!("text {ours:?} against {theirs:?}"));
                }
            }
            if let Some(d) = &step.data {
                let nlst = verb == "NLST";
                let listing = nlst || verb == "LIST";
                if let Some(bytes) = sent.first() {
                    diffs.extend(compare::data(d, bytes, listing, nlst, ebcdic, &mask));
                } else {
                    let empty = matches!(d, DataIn::Text { len: 0, .. }) || matches!(d, DataIn::Hex(b) if b.is_empty());
                    if !empty {
                        diffs.push("no data sent".into());
                    }
                }
            }
        } else {
            diffs.push(format!("codes {ours_codes:?} against {theirs_codes:?} ({})", last(&ours_lines)));
        }
        if diffs.is_empty() {
            out.matched += 1;
        } else {
            out.diffs.push(format!("{}: {}", step.cmd, diffs.join("; ")));
        }
        if flow == Flow::Close {
            break;
        }
    }
    out
}

/// Every transcript under `dir`, by name.
pub fn load(dir: &Path) -> Vec<Transcript> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "txt") {
                let rel = p.strip_prefix(dir).unwrap_or(&p).with_extension("");
                let name = rel.to_string_lossy().replace('\\', "/");
                if let Ok(text) = std::fs::read_to_string(&p) {
                    out.push(transcript::parse(&name, &text));
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The per-category table.
pub fn report(outcomes: &[Outcome]) -> String {
    let mut cats: BTreeMap<&str, (usize, usize, usize, usize, usize)> = BTreeMap::new();
    for o in outcomes {
        let e = cats.entry(o.category()).or_default();
        e.0 += 1;
        e.1 += usize::from(o.passed());
        e.2 += o.compared;
        e.3 += o.matched;
        e.4 += o.deviations;
    }
    let mut s = String::from("| Category | Sessions | Passed | Commands | Matched | Deviations |\n|---|---|---|---|---|---|\n");
    let mut tot = (0, 0, 0, 0, 0);
    for (c, (n, p, cm, m, d)) in &cats {
        let _ = writeln!(s, "| {c} | {n} | {p} | {cm} | {m} | {d} |");
        tot = (tot.0 + n, tot.1 + p, tot.2 + cm, tot.3 + m, tot.4 + d);
    }
    let _ = writeln!(
        s,
        "| all | {} | {} | {} | {} | {} |",
        tot.0, tot.1, tot.2, tot.3, tot.4
    );
    s
}
