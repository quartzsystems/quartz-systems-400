// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped time zone descriptions against the primary oracle's
//! (`tools/conformance/oracle/timzon/zones.tsv`, every zone's Display
//! Time Zone Description Details): the same names in the same order,
//! offsets, abbreviations, message identifiers, alternate names and
//! daylight saving rules, whether the rule came from tzdata or is IBM's.

use sysval::tz::{self, Edge};

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const WEEKS: [&str; 5] = ["First", "Second", "Third", "Fourth", "Last"];

fn edge(e: &Edge) -> String {
    let (h, m, s) = (e.secs / 3600, e.secs % 3600 / 60, e.secs % 60);
    format!(
        "{} {} {} {h:02}:{m:02}:{s:02}",
        MONTHS[usize::from(e.month - 1)],
        DAYS[usize::from(e.weekday)],
        WEEKS[usize::from(e.week - 1)]
    )
}

fn offset(min: i32) -> String {
    let sign = match min.signum() {
        -1 => "-",
        1 => "+",
        _ => " ",
    };
    format!("{sign}{:02}:{:02}", min.abs() / 60, min.abs() % 60).trim().to_string()
}

#[test]
fn shipped_zones_match_the_primary() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/conformance/oracle/timzon/zones.tsv"))
        .expect("zones.tsv");
    let rows: Vec<Vec<&str>> = text.lines().skip(1).map(|l| l.split('\t').collect()).collect();
    let ours = tz::shipped();
    assert_eq!(rows.len(), ours.len());
    let mut diffs = Vec::new();
    for (r, z) in rows.iter().zip(ours) {
        let dst = z.dst.map_or_else(
            || "- -".to_string(),
            |d| format!("{} {} {}", edge(&d.start), edge(&d.end), d.shift),
        );
        let want_dst = if r[10] == "-" {
            "- -".to_string()
        } else {
            format!("{} {} {} {} {} {} {} {} {}", r[10], r[11], r[12], r[13], r[14], r[15], r[16], r[17], r[18])
        };
        let none = |s: &str| if s.is_empty() { "-".to_string() } else { s.to_string() };
        let got = [
            z.name.clone(),
            offset(z.offset),
            z.std_abbr.clone(),
            z.std_msg.clone(),
            none(&z.dst_abbr),
            none(&z.dst_msg),
            z.alt.clone(),
            dst,
        ];
        let want = [r[0], r[1], r[3], r[4], r[6], r[7], r[20], &want_dst].map(str::to_string);
        if got != want {
            diffs.push(format!("{}:\n  want {want:?}\n  got  {got:?}", r[0]));
        }
        assert_eq!(r[19], "0", "{}: year offset", r[0]);
    }
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}

#[test]
fn every_message_has_a_text() {
    for z in tz::shipped() {
        assert!(tz::message_text(&z.std_msg).is_some(), "{}", z.std_msg);
        if !z.dst_msg.is_empty() {
            assert!(tz::message_text(&z.dst_msg).is_some(), "{}", z.dst_msg);
        }
    }
}

#[test]
fn qcpfmsg_holds_the_same_texts() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime/msgf/msgs/qcpfmsg.toml"))
        .expect("qcpfmsg.toml")
        .replace("\r\n", "\n");
    for (id, text) in tz::MESSAGES {
        let entry = format!("id = \"{id}\"\nsev = 0\ntext = \"{text}\"\n");
        assert!(src.contains(&entry), "{id} differs in qcpfmsg.toml");
    }
}
