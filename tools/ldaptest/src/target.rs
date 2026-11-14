// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command line: which server, which scenarios, which identities.

use std::path::PathBuf;
use std::time::Duration;

use crate::Error;

/// The usage text.
pub const USAGE: &str = "\
usage: ldaptest HOST [options]                 run the scenarios
       ldaptest oracle [--out FILE] [options]  the anonymous read-only scenarios against
                                               the primary (QS_ORACLE_HOST), masked
       ldaptest bench HOST [--binds N] [--entries M] [options]
       ldaptest ldif gen N PARENT              N generated entries as LDIF on stdout
       ldaptest --list
options:
  --port N (389)  --ldaps-port N (636)  --timeout SECS (10)
  --suite read,controls,ext,write,acl,projected,ldif (default: all)  --only A,B
  --admin-dn DN (cn=Administrator; password from LDAPTEST_ADMIN_PW)
  --suffix DN          the suffix the write scenarios work under
  --profile NAME       a profile to bind as (password from LDAPTEST_PROFILE_PW)
  --disabled-profile NAME  a *DISABLED profile (password from LDAPTEST_DISABLED_PW)
  --system NAME        the projected backend's os400-sys value (default: the root DSE's)
  --ca FILE            trust this PEM certificate (otherwise any certificate is taken)
  --client-cert FILE --client-key FILE   PEM, for SASL EXTERNAL
  --entries N (200)    entries the paged, sort, LDIF and bench scenarios make
  --page N (50)        the page size";

/// What to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// The scenarios.
    Run,
    /// The read-only scenarios against the primary, recorded.
    Oracle,
    /// Timings.
    Bench,
    /// Generated LDIF on stdout.
    LdifGen,
    /// The scenarios' names.
    List,
}

/// A group of scenarios.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Suite {
    /// The root DSE, the schema, anonymous searches and compares.
    Read,
    /// Controls.
    Controls,
    /// Extended operations, TLS and SASL.
    Ext,
    /// Add, modify, modify DN, delete.
    Write,
    /// Access control.
    Acl,
    /// The projected backend (`os400-sys=`).
    Projected,
    /// LDIF import and export.
    Ldif,
}

impl Suite {
    /// Every suite.
    pub const ALL: [Suite; 7] = [
        Suite::Read,
        Suite::Controls,
        Suite::Ext,
        Suite::Write,
        Suite::Acl,
        Suite::Projected,
        Suite::Ldif,
    ];

    /// Its name on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Suite::Read => "read",
            Suite::Controls => "controls",
            Suite::Ext => "ext",
            Suite::Write => "write",
            Suite::Acl => "acl",
            Suite::Projected => "projected",
            Suite::Ldif => "ldif",
        }
    }

    fn parse(s: &str) -> Result<Suite, Error> {
        Suite::ALL
            .into_iter()
            .find(|x| x.name() == s)
            .ok_or_else(|| Error::Usage(format!("no suite {s}")))
    }
}

/// Everything the command line says.
#[derive(Clone, Debug)]
pub struct Target {
    /// What to do.
    pub command: Command,
    /// The server.
    pub host: String,
    /// Its LDAP port.
    pub port: u16,
    /// Its LDAPS port.
    pub ldaps_port: u16,
    /// Per reply.
    pub timeout: Duration,
    /// The suites to run.
    pub suites: Vec<Suite>,
    /// Only these scenarios, whatever the suites.
    pub only: Vec<String>,
    /// The administrator's DN.
    pub admin_dn: String,
    /// Its password.
    pub admin_pw: Option<String>,
    /// The suffix the write scenarios use.
    pub suffix: Option<String>,
    /// A profile and its password.
    pub profile: Option<(String, Option<String>)>,
    /// A disabled profile and its password.
    pub disabled: Option<(String, Option<String>)>,
    /// `os400-sys=` value.
    pub system: Option<String>,
    /// A trusted certificate.
    pub ca: Option<PathBuf>,
    /// A client certificate and key.
    pub client: Option<(PathBuf, PathBuf)>,
    /// Entries made by the paged, sort, LDIF and bench scenarios.
    pub entries: usize,
    /// Page size.
    pub page: i32,
    /// Binds for `bench`.
    pub binds: usize,
    /// Where `oracle` writes.
    pub out: Option<PathBuf>,
    /// `ldif gen`: the count and the parent DN.
    pub generate: Option<(usize, String)>,
}

impl Target {
    /// Parses `args`, reading passwords and the oracle's host through `env`.
    pub fn from_args(
        args: &[String],
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Target, Error> {
        let mut t = Target {
            command: Command::Run,
            host: String::new(),
            port: 389,
            ldaps_port: 636,
            timeout: Duration::from_secs(10),
            suites: Suite::ALL.to_vec(),
            only: Vec::new(),
            admin_dn: "cn=Administrator".into(),
            admin_pw: None,
            suffix: None,
            profile: None,
            disabled: None,
            system: None,
            ca: None,
            client: None,
            entries: 200,
            page: 50,
            binds: 1000,
            out: None,
            generate: None,
        };
        let mut pos: Vec<&str> = Vec::new();
        let mut cert: Option<PathBuf> = None;
        let mut key: Option<PathBuf> = None;
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut val = || {
                it.next()
                    .map(String::as_str)
                    .ok_or_else(|| Error::Usage(format!("{a} needs a value")))
            };
            match a.as_str() {
                "--list" => t.command = Command::List,
                "--port" => t.port = num(a, val()?)?,
                "--ldaps-port" => t.ldaps_port = num(a, val()?)?,
                "--timeout" => t.timeout = Duration::from_secs(num(a, val()?)?),
                "--suite" => {
                    t.suites = val()?
                        .split(',')
                        .map(Suite::parse)
                        .collect::<Result<_, _>>()?;
                }
                "--only" => t.only = val()?.split(',').map(str::to_owned).collect(),
                "--admin-dn" => val()?.clone_into(&mut t.admin_dn),
                "--suffix" => t.suffix = Some(val()?.to_owned()),
                "--profile" => {
                    t.profile = Some((val()?.to_ascii_uppercase(), env("LDAPTEST_PROFILE_PW")))
                }
                "--disabled-profile" => {
                    t.disabled = Some((val()?.to_ascii_uppercase(), env("LDAPTEST_DISABLED_PW")));
                }
                "--system" => t.system = Some(val()?.to_owned()),
                "--ca" => t.ca = Some(val()?.into()),
                "--client-cert" => cert = Some(val()?.into()),
                "--client-key" => key = Some(val()?.into()),
                "--entries" => t.entries = num(a, val()?)?,
                "--page" => t.page = num(a, val()?)?,
                "--binds" => t.binds = num(a, val()?)?,
                "--out" => t.out = Some(val()?.into()),
                s if s.starts_with("--") => {
                    return Err(Error::Usage(format!("unknown option {s}")));
                }
                s => pos.push(s),
            }
        }
        match (cert, key) {
            (Some(c), Some(k)) => t.client = Some((c, k)),
            (None, None) => {}
            _ => {
                return Err(Error::Usage(
                    "--client-cert and --client-key go together".into(),
                ));
            }
        }
        if t.command == Command::List {
            return Ok(t);
        }
        match pos.as_slice() {
            ["oracle"] => {
                t.command = Command::Oracle;
                t.host = env("QS_ORACLE_HOST")
                    .filter(|h| !h.is_empty())
                    .ok_or_else(|| Error::Usage("oracle: QS_ORACLE_HOST is not set".into()))?;
                // Anonymous only: nothing that could fail a sign-on, nothing written.
                t.admin_pw = None;
                t.profile = None;
                t.disabled = None;
                t.suffix = None;
                t.client = None;
            }
            ["bench", host] => {
                t.command = Command::Bench;
                (*host).clone_into(&mut t.host);
            }
            ["ldif", "gen", n, parent] => {
                t.command = Command::LdifGen;
                t.generate = Some((num("N", n)?, (*parent).to_owned()));
            }
            [host] => (*host).clone_into(&mut t.host),
            [] => return Err(Error::Usage("no host".into())),
            _ => return Err(Error::Usage(format!("unexpected arguments {pos:?}"))),
        }
        if t.command != Command::Oracle {
            t.admin_pw = env("LDAPTEST_ADMIN_PW").filter(|p| !p.is_empty());
        }
        if t.page <= 0 {
            return Err(Error::Usage("--page must be at least 1".into()));
        }
        Ok(t)
    }

    /// `ldap://host:port`.
    pub fn url(&self) -> String {
        format!("ldap://{}:{}", self.host, self.port)
    }

    /// `ldaps://host:port`.
    pub fn ldaps_url(&self) -> String {
        format!("ldaps://{}:{}", self.host, self.ldaps_port)
    }

    /// Whether this is a recorded run against the primary.
    pub fn is_oracle(&self) -> bool {
        self.command == Command::Oracle
    }
}

fn num<T: std::str::FromStr>(what: &str, s: &str) -> Result<T, Error> {
    s.parse()
        .map_err(|_| Error::Usage(format!("{what}: not a number: {s}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    fn env(k: &str) -> Option<String> {
        match k {
            "QS_ORACLE_HOST" => Some("oracle.example".into()),
            "LDAPTEST_ADMIN_PW" => Some("secret".into()),
            "LDAPTEST_PROFILE_PW" => Some("pw".into()),
            _ => None,
        }
    }

    #[test]
    fn run_with_options() {
        let t = Target::from_args(
            &args("h --port 1389 --suite read,acl --suffix dc=t --profile qstest"),
            &env,
        )
        .unwrap();
        assert_eq!(t.command, Command::Run);
        assert_eq!(t.url(), "ldap://h:1389");
        assert_eq!(t.suites, [Suite::Read, Suite::Acl]);
        assert_eq!(t.admin_pw.as_deref(), Some("secret"));
        assert_eq!(t.profile, Some(("QSTEST".into(), Some("pw".into()))));
    }

    #[test]
    fn oracle_is_anonymous() {
        let t = Target::from_args(&args("oracle --suffix dc=t --profile qstest"), &env).unwrap();
        assert_eq!(t.command, Command::Oracle);
        assert_eq!(t.host, "oracle.example");
        assert!(t.admin_pw.is_none() && t.profile.is_none() && t.suffix.is_none());
        assert!(Target::from_args(&args("oracle"), &|_| None).is_err());
    }

    #[test]
    fn usage_errors() {
        assert!(Target::from_args(&args(""), &env).is_err());
        assert!(Target::from_args(&args("h --suite nope"), &env).is_err());
        assert!(Target::from_args(&args("h --port"), &env).is_err());
        assert!(Target::from_args(&args("h --client-cert c"), &env).is_err());
        assert!(Target::from_args(&args("h --page 0"), &env).is_err());
        let g = Target::from_args(&args("ldif gen 5 ou=x"), &env).unwrap();
        assert_eq!(g.generate, Some((5, "ou=x".into())));
    }
}
