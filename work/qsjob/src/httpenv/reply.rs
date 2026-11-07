// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The instances' jobs and the commands that start and end them: the
//! command strings, a command's outcome as the message row the product
//! shows, and the instance jobs among the active jobs.

use httpadmin::env::{EndHow, HttpJob};
use qsweb::msg::{Kind, Message};
use stg::JobInfo;

/// The subsystem instances run in.
pub const SUBSYSTEM: &str = "QHTTPSVR";

/// `STRTCPSVR` for one instance.
pub fn start_command(instance: &str) -> String {
    format!(
        "STRTCPSVR SERVER(*HTTP) HTTPSVR({})",
        instance.trim().to_ascii_uppercase()
    )
}

/// `ENDTCPSVR` for one instance.
pub fn end_command(instance: &str, how: EndHow) -> String {
    let option = match how {
        EndHow::Controlled(delay) => format!("OPTION(*CNTRLD) DELAY({delay})"),
        EndHow::Immediate => "OPTION(*IMMED)".into(),
    };
    format!(
        "ENDTCPSVR SERVER(*HTTP) HTTPSVR({}) {option}",
        instance.trim().to_ascii_uppercase()
    )
}

/// `TCP1A07 HTTP server starting.` as identifier and text.
fn split_line(line: &str) -> Option<(&str, &str)> {
    let (id, rest) = line.trim().split_once(' ')?;
    let b = id.as_bytes();
    let ok = b.len() == 7
        && b[..3].iter().all(u8::is_ascii_alphabetic)
        && b[3..].iter().all(u8::is_ascii_hexdigit);
    ok.then(|| (id, rest.trim()))
}

/// A command's outcome as a message: its escape (identifier and text)
/// as an error, else its first message line as a completion.
pub fn outcome(text: &str, escape: Option<(&str, &str)>) -> Result<Message, Message> {
    if let Some((id, t)) = escape {
        let (id, t) = match split_line(t) {
            Some((i, rest)) if i == id => (id, rest),
            _ => (id, t.trim()),
        };
        return Err(Message::new(Kind::Error, id, t));
    }
    let first = text.lines().find_map(split_line);
    Ok(match first {
        Some((id, t)) => Message::new(Kind::Done, id, t),
        None => Message::new(Kind::Done, "", text.trim()),
    })
}

/// The open connections an instance job's function (`CNN-n`) shows.
pub fn connections(function: &str) -> u32 {
    function
        .trim()
        .strip_prefix("CNN-")
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// The jobs of [`SUBSYSTEM`] that run instances (not its monitor), one
/// per instance, the earliest when there are more.
pub fn instance_jobs(active: &[JobInfo]) -> Vec<HttpJob> {
    let mut out: Vec<HttpJob> = Vec::new();
    let mut jobs: Vec<&JobInfo> = active
        .iter()
        .filter(|j| j.subsystem.trim() == SUBSYSTEM && j.kind != 'M')
        .collect();
    jobs.sort_by_key(|j| (j.started, j.number));
    for j in jobs {
        if out.iter().any(|h| h.instance == j.name) {
            continue;
        }
        out.push(HttpJob {
            instance: j.name.clone(),
            number: format!("{:06}", j.number),
            user: j.user.clone(),
            started: j.started,
            connections: connections(&j.function),
        });
    }
    out.sort_by(|a, b| a.instance.cmp(&b.instance));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(name: &str, number: u32, sbs: &str, kind: char, started: u64) -> JobInfo {
        JobInfo {
            name: name.into(),
            user: "QTMHHTTP".into(),
            number,
            kind,
            subsystem: sbs.into(),
            started,
            ..JobInfo::default()
        }
    }

    #[test]
    fn commands_and_outcomes() {
        assert_eq!(
            start_command("web1"),
            "STRTCPSVR SERVER(*HTTP) HTTPSVR(WEB1)"
        );
        assert_eq!(
            end_command("WEB1", EndHow::Controlled(30)),
            "ENDTCPSVR SERVER(*HTTP) HTTPSVR(WEB1) OPTION(*CNTRLD) DELAY(30)"
        );
        assert!(end_command("WEB1", EndHow::Immediate).ends_with("OPTION(*IMMED)"));
        let ok = outcome("TCP1A07 HTTP server starting.\n", None).unwrap();
        assert_eq!(
            (ok.kind, ok.id.as_str(), ok.text.as_str()),
            (Kind::Done, "TCP1A07", "HTTP server starting.")
        );
        let e = outcome("", Some(("CPF9801", "CPF9801 Object not found."))).unwrap_err();
        assert_eq!(
            (e.kind, e.id.as_str(), e.text.as_str()),
            (Kind::Error, "CPF9801", "Object not found.")
        );
        assert_eq!(
            outcome("", Some(("CPD0043", "Keyword OPTION not valid.")))
                .unwrap_err()
                .text,
            "Keyword OPTION not valid."
        );
    }

    #[test]
    fn instance_jobs_from_the_table() {
        let active = [
            job("QHTTPSVR", 10, "QHTTPSVR", 'M', 1),
            JobInfo {
                function: "CNN-17".into(),
                ..job("WEB1", 30, "QHTTPSVR", 'B', 5)
            },
            job("ADMIN", 20, "QHTTPSVR", 'B', 2),
            job("WEB1", 31, "QHTTPSVR", 'B', 6),
            job("QTVTELNET", 40, "QSYSWRK", 'B', 1),
        ];
        let j = instance_jobs(&active);
        let names: Vec<(&str, &str)> = j
            .iter()
            .map(|h| (h.instance.as_str(), h.number.as_str()))
            .collect();
        assert_eq!(names, [("ADMIN", "000020"), ("WEB1", "000030")]);
        assert_eq!(j[0].qualified(), "000020/QTMHHTTP/ADMIN");
        assert_eq!((j[0].connections, j[1].connections), (0, 17));
    }
}
