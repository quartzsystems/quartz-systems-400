// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signing on: `USER`, `PASS` (through `QIBM_QTMF_SVR_LOGON` when it
//! has a program, which is also the only way to anonymous FTP), `REIN`.

use regfac::ExitRequest;
use regfac::formats::tcp::{Logon, app, logon_out};

use super::{SVR_LOGON, Session};
use crate::host::{Host, Profile};
use crate::names;
use crate::net::Net;
use crate::reply::Reply;

/// The commands a session takes before it signs on.
pub fn before_sign_on(verb: &str) -> bool {
    matches!(
        verb,
        "USER"
            | "PASS"
            | "QUIT"
            | "NOOP"
            | "HELP"
            | "SYST"
            | "FEAT"
            | "AUTH"
            | "PBSZ"
            | "PROT"
            | "CCC"
            | "ACCT"
            | "REIN"
            | "SITE"
            | "TYPE"
            | "MODE"
            | "STRU"
            | "PASV"
            | "EPSV"
            | "PORT"
            | "EPRT"
            | "ALLO"
            | "STAT"
    )
}

/// How a sign-on exit program answered.
enum ExitSays {
    /// No program: sign on as usual.
    Nothing,
    /// Refused.
    Reject,
    /// Sign on as `user` with `password` (`None`: no password check),
    /// with these initial current library and home directory.
    Use {
        user: String,
        password: Option<String>,
        curlib: Option<String>,
        home: Option<String>,
    },
}

impl<H: Host> Session<H> {
    pub(super) fn user_cmd(&mut self, arg: &str) -> Reply {
        if arg.is_empty() {
            return Reply::new(501, "USER needs a user name.");
        }
        if self.user.is_some() {
            return Reply::new(501, "Send REIN before signing on as another user.");
        }
        self.pending_user = Some(arg.to_string());
        Reply::new(331, format!("Send the password for {}.", arg.to_ascii_uppercase()))
    }

    pub(super) fn pass_cmd(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let Some(user) = self.pending_user.take() else {
            return Reply::new(503, "Send USER first.");
        };
        let says = self.logon_exit(&user, arg, net);
        let (name, password, curlib, home) = match says {
            ExitSays::Reject => return refused(&user, "an exit program refused it"),
            ExitSays::Nothing => (user.clone(), Some(arg.to_string()), None, None),
            ExitSays::Use {
                user,
                password,
                curlib,
                home,
            } => (user, password, curlib, home),
        };
        let profile = match &password {
            Some(pw) => match self.host.sign_on(&name, pw) {
                Ok(p) => p,
                Err(r) => return refused(&name, &r.text),
            },
            None => name.to_ascii_uppercase(),
        };
        if let Err(e) = self.host.become_user(&profile, password.as_deref()) {
            return refused(&profile, &e);
        }
        let mut p = self.host.profile(&profile).unwrap_or(Profile {
            user: profile.clone(),
            home: "/".into(),
            curlib: "*CRTDFT".into(),
            ccsid: 0,
        });
        if let Some(c) = curlib.filter(|c| !c.trim().is_empty()) {
            p.curlib = c.trim().to_ascii_uppercase();
        }
        if let Some(h) = home.filter(|h| !h.trim().is_empty()) {
            p.home = h.trim().to_string();
        }
        self.start_place(&p);
        let shown = p.user.clone();
        self.user = Some(p);
        Reply::new(230, format!("{shown} signed on."))
    }

    /// The directory a session starts in: the home directory or the
    /// current library, as `CURDIR` says; `NAMEFMT 0` needs a library.
    fn start_place(&mut self, p: &Profile) {
        let home_ok = crate::objects::exists(self.host.ifs(), &p.home);
        let lib = if p.curlib.is_empty() || p.curlib.starts_with('*') {
            "QGPL".to_string()
        } else {
            p.curlib.clone()
        };
        self.cwd = if self.cfg.attrs.starts_home() {
            if home_ok { names::normalize(&p.home) } else { "/".into() }
        } else {
            names::library_dir(&lib)
        };
        self.namefmt = self.cfg.attrs.namefmt_number();
        if self.namefmt == 0 && names::library_of(&self.cwd).is_none() {
            self.namefmt = 1;
        }
    }

    /// `QIBM_QTMF_SVR_LOGON` in the first format with programs.
    fn logon_exit(&mut self, user: &str, password: &str, net: &dyn Net) -> ExitSays {
        for format in ["TCPL0300", "TCPL0200", "TCPL0100"] {
            let auth = if format == "TCPL0300" {
                password.as_bytes().to_vec()
            } else {
                ccsid::to_ebcdic(password).unwrap_or_else(|_| password.as_bytes().to_vec())
            };
            let req = ExitRequest::Logon {
                format: format.into(),
                logon: Logon {
                    app: app::FTP_SERVER,
                    user_id: user.to_string(),
                    auth,
                    auth_ccsid: 1208,
                    client: net.peer().ip().to_string(),
                    app_info: Vec::new(),
                },
            };
            let a = self.host.exit(SVR_LOGON, &req);
            if a.called.is_empty() {
                if a.error.is_some() {
                    return ExitSays::Reject;
                }
                continue;
            }
            let out = logon_out(format, &a.outputs);
            let text = |b: &[u8]| -> String {
                if format == "TCPL0300" && out.password_ccsid != 37 && out.password_ccsid != 0 {
                    String::from_utf8_lossy(b).trim_end().to_string()
                } else {
                    ccsid::from_ebcdic(b).trim_end().to_string()
                }
            };
            let returned = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
            let curlib = |rc: i32| (rc >= 4).then(|| out.curlib.clone()).and_then(|c| returned(&c));
            let home = |rc: i32| (rc >= 4).then(|| out.home.clone()).and_then(|h| returned(&h));
            return match out.rc {
                1 | 4 => ExitSays::Use {
                    user: user.to_string(),
                    password: Some(password.to_string()),
                    curlib: curlib(out.rc),
                    home: home(out.rc),
                },
                2 | 5 => ExitSays::Use {
                    user: returned(&out.user).unwrap_or_else(|| user.to_string()),
                    password: Some(text(&out.password)),
                    curlib: curlib(out.rc),
                    home: home(out.rc),
                },
                3 | 6 => match returned(&out.user) {
                    Some(u) => ExitSays::Use {
                        user: u,
                        password: None,
                        curlib: curlib(out.rc),
                        home: home(out.rc),
                    },
                    None => ExitSays::Reject,
                },
                _ => ExitSays::Reject,
            };
        }
        ExitSays::Nothing
    }

    pub(super) fn rein(&mut self) -> Reply {
        self.ended();
        self.pending_user = None;
        self.namefmt = self.cfg.attrs.namefmt_number();
        self.listfmt = self.cfg.attrs.listfmt_number();
        self.cwd = "/".into();
        Reply::new(220, "Session reset; sign on again.")
    }
}

fn refused(user: &str, why: &str) -> Reply {
    let why = why.trim().trim_end_matches('.');
    Reply::new(530, format!("Sign-on of {} refused: {why}.", user.to_ascii_uppercase()))
}
