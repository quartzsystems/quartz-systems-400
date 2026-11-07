// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `accessweb::env::download::Download` as the user: the files of
//! `/QQS/ProdData/Access/Web/download` (the storage service keeps the
//! shipped ones there, `osobjects::afwdl`), described by its
//! `INDEX.txt` (section, file, label and note, tab-separated; a file it
//! does not name shows under Other files), and the connection details
//! of this system made on the spot.

use accessweb::env::download::{DIR, DlFile, Download};
use ifs::client::{IfsClient, read_all};
use ifs::ops::OpenArgs;
use ifs::ops::flags::O_RDONLY;
use qsweb::msg::{Kind, Message};
use stg::remote::RemoteIfs;

use super::Link;
use crate::work::web::bridge::Main;

/// The description of the directory.
const INDEX: &str = "INDEX.txt";
/// The connection details, made on the spot.
const DETAILS: &str = "connection.txt";

/// The download directory.
pub struct LiveDownload {
    /// The job's thread.
    pub link: Link,
    /// The system name.
    pub system: String,
}

fn read_file(m: &Main, path: &str) -> Result<Vec<u8>, Message> {
    let c = RemoteIfs::new(m.link.clone());
    let fd = c.open(path, OpenArgs::new(O_RDONLY)).map_err(|e| Message::new(Kind::Error, &e.message_id(), e.text()))?;
    let r = read_all(&c, fd).map_err(|e| Message::new(Kind::Error, &e.message_id(), e.text()));
    let _ = c.close(fd);
    r
}

/// The connection details of `system`.
fn details(system: &str) -> String {
    format!(
        "Connecting to {system}\n\n\
         Database (JDBC and ODBC host server): {system}, port 8471 (9471 with TLS)\n\
         Remote database (DRDA): {system}, port 446, relational database {system}\n\
         QAP (QS/400 Access for Desktop): {system}, port 2320, TLS 1.3\n\
         5250 (Telnet): {system}, port 23 (992 with TLS)\n\
         Access for Web: http://{system}:{}/qs400/accessweb/ (https on {})\n\n\
         Sign on with your user profile and password on {system}.\n",
        accessweb::PORT,
        accessweb::TLS_PORT
    )
}

impl Download for LiveDownload {
    fn files(&self, user: &str) -> Vec<DlFile> {
        let mut files = self.link.read(user, |m| {
            let c = RemoteIfs::new(m.link.clone());
            let Ok(entries) = c.read_dir(DIR) else {
                return Vec::new();
            };
            let index = read_file(m, &format!("{DIR}/{INDEX}")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            let size = |f: &str| c.stat(&format!("{DIR}/{f}"), true).map_or(0, |s| s.size);
            let present: Vec<String> = entries.into_iter().map(|e| e.name).filter(|n| n != INDEX && !n.starts_with('.')).collect();
            let mut out = Vec::new();
            for line in index.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
                let f: Vec<&str> = line.split('\t').collect();
                if f.len() < 3 || !present.iter().any(|p| p == f[1]) {
                    continue;
                }
                out.push(DlFile {
                    section: f[0].trim().into(),
                    file: f[1].trim().into(),
                    label: f[2].trim().into(),
                    size: size(f[1]),
                    note: f.get(3).map_or("", |n| n.trim()).into(),
                });
            }
            for p in &present {
                if !out.iter().any(|d| d.file == *p) {
                    out.push(DlFile {
                        section: "Other files".into(),
                        file: p.clone(),
                        label: p.clone(),
                        size: size(p),
                        note: String::new(),
                    });
                }
            }
            out
        });
        if !files.iter().any(|f| f.file == DETAILS) {
            files.push(DlFile {
                section: "Data".into(),
                file: DETAILS.into(),
                label: format!("ODBC / JDBC connection details for {} (text)", self.system),
                size: details(&self.system).len() as u64,
                note: String::new(),
            });
        }
        files
    }

    fn read(&self, user: &str, file: &str) -> Result<Vec<u8>, Message> {
        if file.contains('/') || file.starts_with('.') {
            return Err(Message::new(Kind::Error, "CPFA0A9", "Object not found."));
        }
        let path = format!("{DIR}/{file}");
        let r = self.link.read(user, move |m| Some(read_file(m, &path)));
        match r {
            Some(Ok(b)) => Ok(b),
            _ if file == DETAILS => Ok(details(&self.system).into_bytes()),
            Some(Err(e)) => Err(e),
            None => Err(super::gone()),
        }
    }
}
