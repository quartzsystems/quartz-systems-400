// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `rexec [-n] [-u user] [-p password] [-P port] host command...`: run
//! a command through the remote execution server (port 512). The user
//! and password come from the options, else the `machine` entry for the
//! host in `$HOME/.netrc`; the user defaults to the job's. Standard
//! input goes to the command (`-n`: none) and its output comes back.

use std::sync::Arc;

use qsh::sys::stream::write_all;
use qsh::sys::{Fd, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::{copy, read_input, stdin};
use crate::common::{complain, usage};

const NAME: &str = "rexec";

/// The REXEC port.
pub const PORT: u16 = 512;

/// The login and password `.netrc` gives a host.
pub fn netrc(text: &str, host: &str) -> (Option<String>, Option<String>) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;
    let mut mine = false;
    let (mut login, mut pass) = (None, None);
    let mut default: (Option<String>, Option<String>) = (None, None);
    let mut in_default = false;
    while i < words.len() {
        match words[i] {
            "machine" => {
                if mine {
                    break;
                }
                in_default = false;
                mine = words.get(i + 1).is_some_and(|h| h.eq_ignore_ascii_case(host));
                i += 2;
            }
            "default" => {
                if mine {
                    break;
                }
                in_default = true;
                i += 1;
            }
            "login" | "password" => {
                let v = words.get(i + 1).map(ToString::to_string);
                let is_login = words[i] == "login";
                if mine {
                    if is_login { login = v } else { pass = v }
                } else if in_default {
                    if is_login { default.0 = v } else { default.1 = v }
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    if mine { (login, pass) } else { default }
}

fn field(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
}

/// Run `rexec`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "nu:l:p:P:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let Some((host, cmd)) = p.operands.split_first() else {
        return usage(cx, NAME, "A host and a command are needed.");
    };
    if cmd.is_empty() {
        return usage(cx, NAME, "A host and a command are needed.");
    }
    let port = match p.value('P') {
        Some(v) => match v.parse::<u16>() {
            Ok(n) if n > 0 => n,
            _ => return usage(cx, NAME, &format!("The port {v} is not valid.")),
        },
        None => PORT,
    };
    let rc = cx
        .getenv("HOME")
        .map(|h| format!("{h}/.netrc"))
        .and_then(|f| read_input(cx, &f).ok())
        .map(|b| netrc(&String::from_utf8_lossy(&b), host))
        .unwrap_or_default();
    let user = p
        .value('u')
        .or(p.value('l'))
        .map(ToString::to_string)
        .or(rc.0)
        .or_else(|| cx.sys.credentials().ok().map(|c| c.user))
        .or_else(|| cx.getenv("LOGNAME").map(ToString::to_string))
        .unwrap_or_default();
    let Some(pass) = p.value('p').map(ToString::to_string).or(rc.1) else {
        complain(
            cx,
            NAME,
            "A password is needed: give -p or a machine entry in $HOME/.netrc.",
        );
        return 1;
    };
    let conn = match cx.sys.tcp_connect(host, port) {
        Ok(c) => c,
        Err(e) => {
            complain(cx, NAME, &format!("{host}: {}", e.errno.text()));
            return 1;
        }
    };
    let mut req = Vec::new();
    field(&mut req, "0");
    field(&mut req, &user);
    field(&mut req, &pass);
    field(&mut req, &cmd.join(" "));
    if let Err(e) = write_all(&*conn, &req) {
        complain(cx, NAME, &format!("{host}: {}", crate::common::io::errno_text(&e)));
        return 1;
    }
    let mut first = [0u8; 1];
    match conn.read(&mut first) {
        Ok(1) if first[0] == 0 => {}
        Ok(1) => {
            let rest = qsh::sys::stream::read_to_end(&*conn).unwrap_or_default();
            let text = String::from_utf8_lossy(&rest);
            complain(cx, NAME, &format!("{host}: {}", text.trim_end()));
            return 1;
        }
        _ => {
            complain(cx, NAME, &format!("{host}: The server ended the connection."));
            return 1;
        }
    }
    // Standard input goes over on its own thread, then the write half
    // ends so the command sees end of file.
    let sender: Option<std::thread::JoinHandle<()>> = if p.has('n') {
        let _ = conn.shutdown_write();
        None
    } else {
        let input: Fd = stdin(cx);
        let c = Arc::clone(&conn);
        Some(std::thread::spawn(move || {
            let _ = copy(&input, &c);
            let _ = c.shutdown_write();
        }))
    };
    let status = match cx.fd(1) {
        Some(o) => match copy(&conn, o) {
            Ok(_) => 0,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::BrokenPipe {
                    complain(cx, NAME, &format!("standard output: {}", crate::common::io::errno_text(&e)));
                }
                1
            }
        },
        None => 1,
    };
    drop(sender);
    status
}

#[cfg(test)]
mod tests {
    use super::netrc;

    #[test]
    fn netrc_entries() {
        let t = "machine a login u1 password p1\nmachine b login u2\ndefault login d password dp";
        assert_eq!(netrc(t, "A"), (Some("u1".into()), Some("p1".into())));
        assert_eq!(netrc(t, "b"), (Some("u2".into()), None));
        assert_eq!(netrc(t, "c"), (Some("d".into()), Some("dp".into())));
    }
}
