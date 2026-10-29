// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `qsh [-cilsS] [-abCefmntuvx] [-o name] [+o name] [command | file] [arg...]`.

use crate::shell::options::{Opt, Options};

/// How a shell was asked to run.
#[derive(Clone, Debug, Default)]
pub struct Invocation {
    /// `-c`: the command string.
    pub command: Option<String>,
    /// A script file to run.
    pub script: Option<String>,
    /// `$0`.
    pub arg0: String,
    /// The positional parameters.
    pub args: Vec<String>,
    /// `-i`, or interactive by default (no command, no script, `-s` or
    /// nothing, and the caller says input is a terminal).
    pub interactive: Option<bool>,
    /// `-l`: a login shell (reads `/etc/profile` and `$HOME/.profile`).
    pub login: bool,
    /// `-s`: commands come from standard input.
    pub stdin: bool,
    /// Options turned on or off.
    pub options: Vec<(Opt, bool)>,
    /// The session has a terminal (5250 session, SSH TTY).
    pub terminal: bool,
}

impl Invocation {
    /// An interactive login session (`STRQSH`, an SSH login).
    pub fn session() -> Self {
        Self {
            arg0: "qsh".to_string(),
            interactive: Some(true),
            login: true,
            terminal: true,
            ..Self::default()
        }
    }

    /// `qsh -c command`.
    pub fn command(cmd: &str) -> Self {
        Self {
            command: Some(cmd.to_string()),
            arg0: "qsh".to_string(),
            ..Self::default()
        }
    }

    /// Parse `qsh` arguments (`argv[0]` first). The error is the message
    /// detail.
    pub fn parse(argv: &[String]) -> Result<Self, String> {
        let mut inv = Self {
            arg0: argv.first().cloned().unwrap_or_else(|| "qsh".to_string()),
            ..Self::default()
        };
        let mut take_command = false;
        let mut i = 1;
        while i < argv.len() {
            let a = &argv[i];
            if a == "--" || a == "-" {
                i += 1;
                break;
            }
            let on = match a.as_bytes().first() {
                Some(b'-') => true,
                Some(b'+') => false,
                _ => break,
            };
            if a.len() < 2 {
                break;
            }
            for c in a[1..].chars() {
                match c {
                    'c' if on => take_command = true,
                    'l' if on => inv.login = true,
                    'o' => {
                        i += 1;
                        let name = argv.get(i).ok_or("option -o needs a name.")?;
                        let o = Options::by_name(name)
                            .ok_or_else(|| format!("option {name} is not valid."))?;
                        inv.options.push((o, on));
                    }
                    _ => match Options::by_letter(c) {
                        Some(Opt::Interactive) => inv.interactive = Some(on),
                        Some(Opt::Stdin) => inv.stdin = on,
                        Some(o) => inv.options.push((o, on)),
                        None => return Err(format!("option {c} is not valid.")),
                    },
                }
            }
            i += 1;
        }
        let rest = &argv[i.min(argv.len())..];
        if take_command {
            let Some(cmd) = rest.first() else {
                return Err("option -c needs a command string.".to_string());
            };
            inv.command = Some(cmd.clone());
            if let Some(a0) = rest.get(1) {
                inv.arg0.clone_from(a0);
                inv.args = rest[2..].to_vec();
            }
        } else if inv.stdin || rest.is_empty() {
            inv.args = rest.to_vec();
        } else {
            inv.script = Some(rest[0].clone());
            inv.arg0.clone_from(&rest[0]);
            inv.args = rest[1..].to_vec();
        }
        Ok(inv)
    }

    /// Is the shell interactive?
    pub fn is_interactive(&self) -> bool {
        self.interactive
            .unwrap_or(self.command.is_none() && self.script.is_none() && self.terminal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn forms() {
        let i = Invocation::parse(&v(&["qsh", "-c", "echo $0 $1", "name", "a"])).unwrap();
        assert_eq!(i.command.as_deref(), Some("echo $0 $1"));
        assert_eq!(i.arg0, "name");
        assert_eq!(i.args, ["a"]);
        let i = Invocation::parse(&v(&["qsh", "-ex", "-o", "noglob", "s.sh", "1"])).unwrap();
        assert_eq!(i.script.as_deref(), Some("s.sh"));
        assert_eq!(
            i.options,
            [
                (Opt::ErrExit, true),
                (Opt::XTrace, true),
                (Opt::NoGlob, true)
            ]
        );
        assert!(Invocation::parse(&v(&["qsh", "-Z"])).is_err());
    }
}
