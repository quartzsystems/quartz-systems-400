// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Source in stream files for the compile commands (`SRCSTMF`): the
//! text read through the IFS in the job's name (relative paths from the
//! job's current directory), converted from the file's CCSID; the
//! stream files `/COPY` and `/INCLUDE` name, searched in IBM's order;
//! and, for the tests, stream files written.

use ifs::Errno;
use ifs::client::{IfsClient, read_all};
#[cfg(test)]
use ifs::client::write_all;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_RDONLY, O_TEXTDATA};
#[cfg(test)]
use ifs::ops::flags::{O_CREAT, O_TRUNC, O_WRONLY};
use rpgfront::Member;
use rpgfront::reader::STMF;
use rpgfront::source::Record;

use crate::ipl::system::System;
use crate::stgsvc::ifs as service;

/// A stream file's text as UTF-8 (the IFS converts from its CCSID).
///
/// # Errors
///
/// The IFS's error as the escape message and path a command sends.
pub fn read_text(c: &dyn IfsClient, path: &str) -> Result<String, String> {
    let fail = |e: Errno| format!("{} {path}: {}", e.command_message(), e.text());
    let fd = c
        .open(path, OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208))
        .map_err(fail)?;
    let data = read_all(c, fd);
    let _ = c.close(fd);
    let data = data.map_err(fail)?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}

// MIGRATE(phase11): INFOSTMF's PCML is written with it.
/// Write text to a stream file tagged 1208, replacing what was there.
///
/// # Errors
///
/// The IFS's error as a command's message.
#[cfg(test)]
pub fn write_text(c: &dyn IfsClient, path: &str, text: &str) -> Result<(), String> {
    let fail = |e: Errno| format!("{} {path}: {}", e.command_message(), e.text());
    let fd = c
        .open(path, OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID).mode(0o644).conv(1208))
        .map_err(fail)?;
    let r = write_all(c, fd, text.as_bytes());
    let _ = c.close(fd);
    r.map_err(fail)
}

/// The directory part of a path (`""` for a bare name).
#[must_use]
pub fn dir_of(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".into(),
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

/// A stream file's text as the RPG compiler reads a member: records
/// numbered `000100`, `000200` ..., zero dates; the library `*STMF`,
/// the file its directory, the name its path.
#[must_use]
pub fn member_of(path: &str, text: &str) -> Member {
    let records = text
        .replace("\r\n", "\n")
        .lines()
        .enumerate()
        .map(|(i, l)| Record {
            seq: format!("{:06}", (i + 1) * 100),
            date: "000000".into(),
            text: l.trim_end().to_string(),
        })
        .collect();
    Member {
        lib: STMF.into(),
        file: dir_of(path),
        name: path.to_string(),
        ccsid: 1208,
        records,
    }
}

/// Where a `/COPY` of a stream file is looked for, in IBM's order: an
/// absolute path as it is; a relative one in the current directory,
/// then each `INCDIR` directory, then the directory of the source that
/// names it.
#[must_use]
pub fn candidates(target: &str, incdir: &[String], src_dir: &str) -> Vec<String> {
    if target.starts_with('/') {
        return vec![target.to_string()];
    }
    let join = |d: &str| {
        if d.ends_with('/') {
            format!("{d}{target}")
        } else {
            format!("{d}/{target}")
        }
    };
    let mut v = vec![target.to_string()];
    v.extend(incdir.iter().filter(|d| !d.is_empty()).map(|d| join(d)));
    if !src_dir.is_empty() {
        v.push(join(src_dir));
    }
    v.dedup();
    v
}

/// The first candidate that reads, as a member.
pub fn find(read: &mut dyn FnMut(&str) -> Option<String>, target: &str, incdir: &[String], src_dir: &str) -> Option<Member> {
    candidates(target, incdir, src_dir)
        .into_iter()
        .find_map(|p| read(&p).map(|t| member_of(&p, &t)))
}

/// Run `f` against the IFS as the job whose request this is: its
/// principal, its descriptors' owner (so its current directory), its
/// CCSID.
pub fn with_job<T>(system: &mut System, f: impl FnOnce(&dyn IfsClient) -> T) -> T {
    let number = system.current_job;
    let handle = u64::from(number);
    let p = crate::svc::ifs::principal();
    let ccsid = service::job_ccsid(system, number);
    service::with_client(system, handle, service::owner_of(handle), &p, ccsid, f)
}

/// Read a stream file as the job.
///
/// # Errors
///
/// As [`read_text`].
pub fn read_as_job(system: &mut System, path: &str) -> Result<String, String> {
    with_job(system, |c| read_text(c, path))
}

/// The object name a stream file gives when the command names none:
/// its base name before the first dot, upper case, at most ten.
#[must_use]
pub fn default_name(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    let stem = base.split('.').next().unwrap_or(base);
    stem.chars().take(10).collect::<String>().to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_order_is_ibm_s() {
        let inc = vec!["/inc".to_string(), "/more/".to_string()];
        assert_eq!(
            candidates("p.rpgleinc", &inc, "/home/QSTEST"),
            ["p.rpgleinc", "/inc/p.rpgleinc", "/more/p.rpgleinc", "/home/QSTEST/p.rpgleinc"]
        );
        assert_eq!(candidates("/abs/x", &inc, "/src"), ["/abs/x"]);
        assert_eq!(dir_of("/a/b.c"), "/a");
        assert_eq!(dir_of("/b"), "/");
        assert_eq!(default_name("/home/QSTEST/hello_world.rpgle"), "HELLO_WORL");
    }

    #[test]
    fn a_stream_file_as_a_member() {
        let m = member_of("/s/x.rpgle", "**free\r\ndsply 'hi';\n");
        assert_eq!((m.lib.as_str(), m.file.as_str(), m.name.as_str()), (STMF, "/s", "/s/x.rpgle"));
        assert_eq!(m.records[1].seq, "000200");
        assert_eq!(m.records[1].text, "dsply 'hi';");
    }
}
