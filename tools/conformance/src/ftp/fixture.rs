// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The world the sessions were recorded in (`ftp/fixture.txt`): the
//! account's home with what was in it, its library and files,
//! `QSYSINC`'s files, `/QOpenSys/usr`, as an `ftp::memhost::MemHost`.

use ftp::memhost::MemHost;
use ::ifs::client::IfsClient;

/// The profile the replay signs on as: as long as the account the
/// transcripts mask, so listings keep their layout.
pub const USER: &str = "QSFTPUSR";
/// Its password.
pub const PASSWORD: &str = "QSFTPPWD";

fn attr<'a>(words: &'a [&'a str], key: &str) -> Option<&'a str> {
    words.iter().find_map(|w| w.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

/// The job CCSID the oracle's account had: German, as `TYPE E` and the
/// files its sessions made show (X'BA' for the not sign).
pub const JOB_CCSID: u16 = 273;

/// The host with the fixture loaded for `session`: the rows before any
/// `[name]` section and those of the session's own section.
pub fn world(fixture: &str, session: &str) -> MemHost {
    let mut h = MemHost::new(&[(USER, PASSWORD)]);
    h.ccsid = JOB_CCSID;
    let mut mine = true;
    for line in fixture.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            mine = name == session;
            continue;
        }
        if !mine {
            continue;
        }
        let line = line.replace("{user}", USER);
        let words: Vec<&str> = line.split_whitespace().collect();
        let (kind, what) = (words[0], words.get(1).copied().unwrap_or(""));
        let owner = attr(&words, "owner").unwrap_or(USER).to_string();
        let mode = attr(&words, "mode").and_then(|m| u32::from_str_radix(m, 8).ok()).unwrap_or(0o770);
        let uid = match owner.as_str() {
            "QSYS" => 0,
            "QSECOFR" => 1,
            _ => 100,
        };
        h.act_as("QSECOFR");
        match kind {
            "dir" => {
                let _ = h.ifs.mkdir(what, mode);
                let _ = h.ifs.chmod(what, mode);
                let _ = h.ifs.chown(what, uid, 0);
            }
            "stmf" => {
                let size: usize = attr(&words, "size").and_then(|s| s.parse().ok()).unwrap_or(0);
                let _ = ftp::objects::write_bytes(&h.ifs, what, &vec![b'x'; size], ftp::objects::Write::Replace(819));
                let _ = h.ifs.chmod(what, mode);
                let _ = h.ifs.chown(what, uid, 0);
            }
            "link" => {
                // Made by its owner, who owns the directory it is in.
                h.act_as(&owner);
                let _ = h.ifs.symlink(words.get(2).copied().unwrap_or(""), what);
            }
            "gone" => {
                let _ = h.ifs.remove_tree(what);
            }
            "lib" => h.add_library(what),
            "file" => {
                let (lib, file) = what.split_once('/').unwrap_or(("QGPL", what));
                let len = attr(&words, "rcdlen").and_then(|s| s.parse().ok()).unwrap_or(80);
                let source = attr(&words, "source") == Some("1");
                let data_len = if source { len - 12 } else { len };
                h.add_file(lib, file, data_len, source, 37, &owner);
            }
            "mbr" => {
                let (lib, rest) = what.split_once('/').unwrap_or(("QGPL", what));
                let (file, mbr) = rest.split_once('.').unwrap_or((rest, rest));
                h.add_member(lib, file, mbr, &[]);
            }
            _ => {}
        }
    }
    h.act_as("QSECOFR");
    h
}
