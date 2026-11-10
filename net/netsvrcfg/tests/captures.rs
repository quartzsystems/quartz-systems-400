// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The list formats and the list APIs' errors against the primary's
//! captures (`tools/conformance/oracle/netserver/zlsl/`), byte for byte.

use std::path::PathBuf;

use netsvrcfg::api::{self, MemHost};
use netsvrcfg::fmt::list::{self, List, SPACE_LIST};
use netsvrcfg::fmt::text::fixed;
use netsvrcfg::fmt::zlsl::{self, Level};
use netsvrcfg::{Config, Share, Stats};

fn capture(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/oracle/netserver/zlsl")
        .join(name);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace('\r', "")
}

/// The bytes of the hex dump that follows the line starting `title`.
fn dump(text: &str, title: &str) -> Vec<u8> {
    let mut lines = text.lines().skip_while(|l| !l.starts_with(title)).skip(1);
    // The list information is 64 bytes; the other dumps say their size.
    let declared: Option<usize> = if title.starts_with("list info") {
        Some(64)
    } else {
        title
            .split('(')
            .nth(1)
            .and_then(|t| t.split(' ').next())
            .and_then(|n| n.parse().ok())
    };
    let mut out: Vec<u8> = Vec::new();
    let mut starred = false;
    for l in lines.by_ref() {
        if l == "*" {
            starred = true;
            continue;
        }
        let Some((off, rest)) = l.split_once(' ') else {
            break;
        };
        let Ok(off) = usize::from_str_radix(off, 16) else {
            break;
        };
        if off.is_multiple_of(16) && starred {
            let last: Vec<u8> = out[out.len() - 16..].to_vec();
            while out.len() < off {
                out.extend_from_slice(&last);
            }
        }
        starred = false;
        let hex: String = rest
            .split('|')
            .next()
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        for i in (0..hex.len()).step_by(2) {
            out.push(u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"));
        }
    }
    if starred && let Some(n) = declared {
        let last: Vec<u8> = out[out.len() - 16..].to_vec();
        while out.len() < n {
            out.extend_from_slice(&last);
        }
    }
    out
}

fn section(text: &str, prefix: &str) -> Vec<u8> {
    let title = text
        .lines()
        .find(|l| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix}"))
        .to_string();
    dump(text, &title)
}

/// The primary's configuration as the captures show it.
fn primary() -> Config {
    let mut c = Config::shipped("S062B175");
    c.shares.clear();
    let mut d = Share::file(
        "QDIRSRV",
        "/QIBM/ProdData/OS400/DirSrv",
        "OS/400 -- Directory Services",
    );
    if let netsvrcfg::Kind::File(f) = &mut d.kind {
        f.max_users = i32::MAX;
    }
    let q = Share::file("QIBM", "/QIBM", "IBM Product Directories");
    c.shares.insert(d.name.clone(), d);
    c.shares.insert(q.name.clone(), q);
    for s in [&mut c.attrs.current, &mut c.attrs.pending] {
        s.domain = "SEAWOLF".into();
        s.text = "OS/400".into();
    }
    c
}

fn host() -> MemHost {
    let mut h = MemHost::new("S062B175");
    h.config = Some(primary());
    h
}

fn olst(fmt: &str, qual: &str) -> Result<Vec<u8>, api::ApiError> {
    let parms = vec![
        65535i32.to_be_bytes().to_vec(),
        fixed(fmt, 8),
        fixed(qual, 15),
    ];
    api::call(&mut host(), "QZLSOLST", &parms)
}

#[test]
fn olst_zlsl0100_matches() {
    let cap = capture("olst-ZLSL0100-ALL.txt");
    let got = olst("ZLSL0100", "*ALL").expect("lists");
    assert_eq!(&got[64..], section(&cap, "receiver hex").as_slice());
    let info = section(&cap, "list info hex");
    // Everything but the creation time.
    assert_eq!(got[..17], info[..17]);
    assert_eq!(got[30..64], info[30..64]);
}

#[test]
fn olst_zlsl0101_matches_at_7_1() {
    let cap = capture("olst-ZLSL0101-ALL.txt");
    let c = primary();
    let mut at = 0;
    let mut recs = Vec::new();
    for s in c.shares.values() {
        let r = zlsl::l0101(s, 0, 37, at, Level::V7R1);
        at += r.len();
        recs.extend(r);
    }
    assert_eq!(recs, section(&cap, "receiver hex"));
}

#[test]
fn olst_attribute_formats_match() {
    let c = primary();
    assert_eq!(
        zlsl::l0200(&c),
        section(&capture("olst-ZLSL0200-ALL.txt"), "receiver hex")
    );
    assert_eq!(
        zlsl::l0201(&c, Level::V7R1),
        section(&capture("olst-ZLSL0201-ALL.txt"), "receiver hex")
    );
    let got = olst("ZLSL0200", "").expect("lists");
    assert_eq!(
        &got[64..],
        section(&capture("olst-ZLSL0200-blank.txt"), "receiver hex").as_slice()
    );
}

#[test]
fn olst_zlsl0400_and_0800_match() {
    let st = Stats {
        bytes_sent: 89,
        bytes_received: 385,
        requests: 1,
        response_ms: 12,
        started: 1,
        reset: 1,
        ..Stats::default()
    };
    let rec = zlsl::l0400(&st, false, &|_| "1261030065513".into());
    assert_eq!(
        rec,
        section(&capture("olst-ZLSL0400-ALL.txt"), "receiver hex")
    );
    let got = olst("ZLSL0800", "*ALL").expect("lists");
    assert_eq!(
        &got[64..],
        section(&capture("olst-ZLSL0800-ALL.txt"), "receiver hex").as_slice()
    );
    let one = olst("ZLSL0100", "QIBM").expect("lists");
    assert_eq!(
        &one[64..],
        section(&capture("olst-ZLSL0100-QIBM.txt"), "receiver hex").as_slice()
    );
    let none = olst("ZLSL0100", "NOSUCHSHARE").expect("lists");
    assert_eq!(none.len(), 64);
    let empty = olst("ZLSL0300", "*ALL").expect("lists");
    let info = section(&capture("olst-ZLSL0300-ALL.txt"), "list info hex");
    assert_eq!(empty[..17], info[..17]);
}

/// The `ERRC0100` data a capture's first line shows.
fn error_data(name: &str) -> (String, Vec<u8>) {
    let cap = capture(name);
    let l = cap.lines().nth(1).expect("result line");
    let id = l
        .split("error=")
        .nth(1)
        .expect("error")
        .split(' ')
        .next()
        .expect("id")
        .to_string();
    let hex = l.split("data=").nth(1).expect("data");
    let b = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
        .collect();
    (id, b)
}

#[test]
fn list_errors_match() {
    for (file, fmt, qual) in [
        ("olst-ZLSL9999-ALL.txt", "ZLSL9999", "*ALL"),
        ("olst-ZLSL0400-blank.txt", "ZLSL0400", ""),
        ("olst-ZLSL0900-ALL.txt", "ZLSL0900", "*ALL"),
    ] {
        let e = olst(fmt, qual).expect_err("refused");
        assert_eq!((e.id.to_string(), e.data), error_data(file), "{file}");
    }
    for (file, fmt, qual) in [
        ("lsti-ZLSL9999-ALL.txt", "ZLSL9999", "*ALL"),
        ("lsti-ZLSL0400-blank.txt", "ZLSL0400", ""),
        ("lsti-ZLSL0900-ALL.txt", "ZLSL0900", "*ALL"),
    ] {
        let parms = vec![fixed("ZLSSPC    QTEMP", 20), fixed(fmt, 8), fixed(qual, 15)];
        let e = api::call(&mut host(), "QZLSLSTI", &parms).expect_err("refused");
        assert_eq!((e.id.to_string(), e.data), error_data(file), "{file}");
    }
}

/// The user space from offset 64 as the capture shows it, with the
/// creation stamp the capture has.
fn lsti(fmt: &str) -> (Vec<u8>, Vec<u8>) {
    let cap = capture(&format!("lsti-{fmt}-ALL.txt"));
    let want = section(&cap, "user space hex");
    let stamp = netsvrcfg::fmt::text::text_at(&want, 0x5A, 13);
    let c = primary();
    let mut l = List::default();
    match fmt {
        "ZLSL0800" => {
            l.record_len = 16;
            l.records = c.shares.values().map(zlsl::l0800).collect();
        }
        "ZLSL0100" => {
            l.record_len = zlsl::L0100_SIZE;
            for (i, s) in c.shares.values().enumerate() {
                l.records
                    .push(zlsl::l0100(s, 0, 37, SPACE_LIST + i * zlsl::L0100_SIZE));
            }
        }
        "ZLSL0101" => {
            let mut at = SPACE_LIST;
            for s in c.shares.values() {
                let r = zlsl::l0101(s, 0, 37, at, Level::V7R1);
                at += r.len();
                l.records.push(r);
            }
        }
        _ => l.record_len = zlsl::L0300_SIZE,
    }
    let got = list::space(
        &l,
        fmt,
        &fixed("*ALL", 15),
        &fixed("ZLSSPC    QTEMP", 20),
        &stamp,
    );
    (got, want[0x40..].to_vec())
}

#[test]
fn lsti_user_spaces_match() {
    for f in ["ZLSL0800", "ZLSL0100", "ZLSL0101", "ZLSL0300"] {
        let (got, want) = lsti(f);
        assert_eq!(got, want, "{f}");
    }
}
