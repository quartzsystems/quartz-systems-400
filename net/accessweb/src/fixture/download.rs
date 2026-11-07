// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's download directory: the client pieces of
//! `web/download.html`.

use qsweb::msg::{Kind, Message};

use crate::env::download::{DlFile, Download};

/// Section, file, label, bytes, note.
const FILES: &[(&str, &str, &str, u64, &str)] = &[
    (
        "Printer drivers",
        "driver-3812.zip",
        "QS/400 printer driver for Windows (3812 / AFP emulation)",
        4_404_019,
        "",
    ),
    (
        "Printer drivers",
        "driver-label.zip",
        "QS/400 printer driver for Windows - label printer (PRT02)",
        1_153_434,
        "",
    ),
    (
        "Printer drivers",
        "qs400-ppd.tar.gz",
        "CUPS PPD files for Linux and macOS",
        49_152,
        "",
    ),
    (
        "Terminal",
        "terminal-font.ttf",
        "QS/400 terminal font (TrueType, fixed pitch with 5250 line-drawing glyphs)",
        215_040,
        "",
    ),
    (
        "Terminal",
        "keyboard-map.pdf",
        "Keyboard map for 5250 sessions (PDF)",
        92_160,
        "",
    ),
    (
        "Terminal",
        "sessions.qs5250",
        "5250 session shortcuts for the desktop (.qs5250 file)",
        612,
        "",
    ),
    (
        "Certificates",
        "quartz-ca.pem",
        "Quartz Internal CA certificate (PEM)",
        1_984,
        "install this to trust the TLS certificates on QSHQ400",
    ),
    (
        "Certificates",
        "qshq400.pem",
        "QSHQ400 self-signed certificate (PEM)",
        1_870,
        "",
    ),
    (
        "Documentation",
        "accessweb-guide.pdf",
        "QS/400 Access for Web user guide (PDF)",
        1_887_437,
        "",
    ),
    (
        "Documentation",
        "cl-reference.pdf",
        "CL command reference (PDF)",
        13_002_342,
        "",
    ),
    (
        "Documentation",
        "msg-reference.pdf",
        "Message reference CPF / SQL / HTP / TCP (PDF)",
        9_542_042,
        "",
    ),
    (
        "Data",
        "connection.txt",
        "ODBC / JDBC connection details for QSHQ400 (text)",
        412,
        "",
    ),
];

/// The sample files.
#[derive(Debug, Default)]
pub struct FixtureDownload;

impl Download for FixtureDownload {
    fn files(&self, _user: &str) -> Vec<DlFile> {
        FILES
            .iter()
            .map(|f| DlFile {
                section: f.0.into(),
                file: f.1.into(),
                label: f.2.into(),
                size: f.3,
                note: f.4.into(),
            })
            .collect()
    }

    fn read(&self, _user: &str, file: &str) -> Result<Vec<u8>, Message> {
        match FILES.iter().find(|f| f.1 == file) {
            Some(f) => Ok(format!("{}\n", f.2).into_bytes()),
            None => Err(Message::new(
                Kind::Error,
                "CPFA0A9",
                &format!("Object not found.  Object is {file}."),
            )),
        }
    }
}
