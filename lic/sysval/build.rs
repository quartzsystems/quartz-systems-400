// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Generate the shipped time zone table from `zones.toml` and the IANA
//! tz database in `tzdata/`: each zone that names a `year` gets the
//! daylight saving rule its alternate (IANA) zone follows in that year;
//! the build fails when tzdata and the zone disagree on the offset or
//! the rule cannot be held by a *TIMZON (one start and one end a year).

#[path = "build/tzdata.rs"]
mod tzdata;

use std::fmt::Write as _;
use std::path::Path;

use tzdata::{Edge, TzData};

const SOURCES: [&str; 9] = [
    "africa",
    "antarctica",
    "asia",
    "australasia",
    "europe",
    "northamerica",
    "southamerica",
    "etcetera",
    "backward",
];

const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

/// `MAR SUN LAST 02:00:00`.
fn literal(name: &str, s: &str) -> Edge {
    let f: Vec<&str> = s.split_whitespace().collect();
    assert!(f.len() == 4, "{name}: edge {s}");
    let pos = |list: &[&str], v: &str| {
        list.iter()
            .position(|x| *x == v)
            .unwrap_or_else(|| panic!("{name}: {v}"))
    };
    let week = if f[2] == "LAST" {
        5
    } else {
        f[2].parse().unwrap_or_else(|_| panic!("{name}: {s}"))
    };
    let t: Vec<i64> = f[3].split(':').map(|x| x.parse().expect("time")).collect();
    Edge {
        month: u32::try_from(pos(&MONTHS, f[0])).expect("month") + 1,
        weekday: u32::try_from(pos(&DAYS, f[1])).expect("day"),
        week,
        secs: t[0] * 3600 + t[1] * 60 + t[2],
    }
}

fn edge_src(e: &Edge) -> String {
    format!(
        "Edge {{ month: {}, weekday: {}, week: {}, secs: {} }}",
        e.month, e.weekday, e.week, e.secs
    )
}

fn s<'a>(t: &'a toml::Table, k: &str) -> Option<&'a str> {
    t.get(k).and_then(toml::Value::as_str)
}

fn pair(t: &toml::Table, k: &str) -> Option<(String, String)> {
    let a = t.get(k)?.as_array()?;
    Some((a[0].as_str()?.to_string(), a[1].as_str()?.to_string()))
}

// One pass over the zone table and tzdata, in the order the table is written.
#[allow(clippy::too_many_lines)]
fn main() {
    println!("cargo:rerun-if-changed=zones.toml");
    println!("cargo:rerun-if-changed=tzdata");
    println!("cargo:rerun-if-changed=build/tzdata.rs");
    let mut tz = TzData::default();
    for f in SOURCES {
        let p = Path::new("tzdata").join(f);
        tz.add(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())));
    }
    let version = std::fs::read_to_string("tzdata/version").expect("tzdata/version");
    let doc: toml::Table =
        toml::from_str(&std::fs::read_to_string("zones.toml").expect("zones.toml"))
            .expect("zones.toml parses");
    let messages = doc
        .get("messages")
        .and_then(toml::Value::as_table)
        .expect("[messages]");
    let mut out = String::new();
    writeln!(out, "/// The IANA tz database release the rules came from.\npub const TZDATA_VERSION: &str = {:?};", version.trim())
        .expect("string");
    out.push_str(
        "/// The shipped zones, in `zones.toml` order.\npub static SHIPPED: &[Shipped] = &[\n",
    );
    for entry in doc
        .get("zone")
        .and_then(toml::Value::as_array)
        .expect("[[zone]]")
    {
        let zt = entry.as_table().expect("zone table");
        let name = s(zt, "name").expect("name");
        let offset = zt
            .get("offset")
            .and_then(toml::Value::as_integer)
            .expect("offset");
        let (std_abbr, std_msg) = pair(zt, "std").expect("std");
        let dst = pair(zt, "dst");
        let alt = s(zt, "alt").unwrap_or("");
        let year = zt.get("year").and_then(toml::Value::as_integer);
        let text = |id: &str| {
            messages
                .get(id)
                .and_then(toml::Value::as_str)
                .unwrap_or_else(|| panic!("{name}: no text for {id}"))
        };
        let rule = match (&dst, year, s(zt, "start")) {
            (None, Some(yr), _) => {
                if tz.knows(alt) {
                    let r = tz.rule(alt, yr).unwrap_or_else(|e| panic!("{name}: {e}"));
                    assert!(
                        r.stdoff == offset * 60,
                        "{name}: {alt} is {} in {yr}, not {offset} minutes",
                        r.stdoff
                    );
                }
                None
            }
            (None, None, _) => None,
            (Some(_), Some(yr), None) => {
                let r = tz.rule(alt, yr).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert!(
                    r.stdoff == offset * 60,
                    "{name}: {alt} is {} in {yr}, not {offset} minutes",
                    r.stdoff
                );
                let (a, b, shift) = r
                    .dst
                    .unwrap_or_else(|| panic!("{name}: {alt} keeps no daylight saving in {yr}"));
                Some((a, b, shift / 60, format!("Source::Tzdata({yr})")))
            }
            (Some(_), None, Some(st)) => {
                let end = s(zt, "end").unwrap_or_else(|| panic!("{name}: start without end"));
                let shift = zt
                    .get("shift")
                    .and_then(toml::Value::as_integer)
                    .expect("shift");
                Some((
                    literal(name, st),
                    literal(name, end),
                    shift,
                    "Source::Ibm".to_string(),
                ))
            }
            _ => panic!("{name}: a zone with dst needs `year` or `start`/`end`, not both"),
        };
        let dst_src = dst.as_ref().map_or_else(
            || "None".to_string(),
            |(a, m)| format!("Some(({a:?}, {m:?}, {:?}))", text(m)),
        );
        let rule_src = rule.map_or_else(
            || "None".to_string(),
            |(a, b, shift, src)| {
                format!(
                    "Some(ShippedRule {{ start: {}, end: {}, shift: {shift}, source: {src} }})",
                    edge_src(&a),
                    edge_src(&b)
                )
            },
        );
        writeln!(
            out,
            "    Shipped {{ name: {name:?}, offset: {offset}, std: ({std_abbr:?}, {std_msg:?}, {:?}), dst: {dst_src}, alt: {alt:?}, rule: {rule_src} }},",
            text(&std_msg)
        )
        .expect("string");
    }
    out.push_str(
        "];\n/// The message texts by identifier.\npub static MESSAGES: &[(&str, &str)] = &[\n",
    );
    for (k, v) in messages {
        writeln!(out, "    ({k:?}, {:?}),", v.as_str().expect("text")).expect("string");
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("zones.rs");
    std::fs::write(dest, out).expect("write zones.rs");
}
