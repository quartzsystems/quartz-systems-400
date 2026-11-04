// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE RPG compiler's diagnostics in RPG/400's terms. RPG/400 and
//! ILE RPG number most of their messages alike (`RNF7031` is
//! `QRG7031`); the table holds the ones RPG/400 reports differently or
//! not at all, and where its listing prints each. The texts are ours.

use super::Place;

/// How an ILE RPG diagnostic is reported: `None` drops it.
struct Map {
    rnf: &'static str,
    qrg: Option<&'static str>,
    severity: Option<u8>,
    place: Place,
}

const fn m(rnf: &'static str, qrg: Option<&'static str>, severity: Option<u8>, place: Place) -> Map {
    Map { rnf, qrg, severity, place }
}

/// The ones that are not `QRG` of the same number, severity and place
/// after the statement.
const TABLE: &[Map] = &[
    m("RNF7031", Some("QRG7031"), None, Place::Xref),
    m("RNF7030", Some("QRG7030"), None, Place::Xref),
    m("RNF7086", Some("QRG7086"), None, Place::Additional),
    // ILE RPG only: an overflow indicator assigned, a format no
    // operation names, an output record without spacing, the
    // indicator area of a display file.
    m("RNF2318", None, None, Place::Source),
    m("RNF7066", None, None, Place::Source),
    m("RNF6011", None, None, Place::Source),
    m("RNF7089", None, None, Place::Source),
    m("RNF3530", None, None, Place::Source),
];

/// The RPG/400 identifier, severity and place of an ILE RPG diagnostic.
#[must_use]
pub fn qrg(rnf: &str, severity: u8) -> Option<(String, u8, Place)> {
    if let Some(e) = TABLE.iter().find(|e| e.rnf == rnf) {
        return e.qrg.map(|q| (q.to_string(), e.severity.unwrap_or(severity), e.place));
    }
    let num = rnf.strip_prefix("RNF")?;
    Some((format!("QRG{num}"), severity, Place::Source))
}

/// The diagnostic a conversion message stands for.
#[must_use]
pub fn conversion_qrg(rnm: &str) -> &'static str {
    match rnm {
        "RNM0507" => "QRG2001",
        "RNM0501" => "QRG3001",
        _ => "QRG5014",
    }
}

/// Our text of an RPG/400 diagnostic.
#[must_use]
pub fn text(qrg: &str) -> &'static str {
    match qrg {
        "QRG7031" => "Name or indicator is never used.",
        "QRG7030" => "Name or indicator is not defined.",
        "QRG7086" => "Records of the file are read in blocks.",
        "QRG7064" => "The file is not used for output.",
        "QRG5014" => "The operation code is not valid.",
        "QRG2001" => "The file continuation option is not valid.",
        "QRG3001" => "The data format is not supported.",
        "QRG9001" => "The program was not created.",
        _ => "See the ILE RPG diagnostic of the same number.",
    }
}
