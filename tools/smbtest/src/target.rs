// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the scenarios run against, from the command line.

use crate::Error;

/// The server, the account and where on the share the scenarios may write.
#[derive(Clone, Debug)]
pub struct Target {
    /// Host name or IP address.
    pub host: String,
    /// TCP port (445 on a real server).
    pub port: u16,
    /// The user profile.
    pub user: String,
    /// Its password.
    pub password: String,
    /// The file share.
    pub share: String,
    /// A directory on the share the scenarios may write under (`\`-separated,
    /// relative to the share; empty for the share's root).
    pub scratch: String,
    /// A print share, when there is one to list.
    pub print_share: Option<String>,
    /// Bytes for the timed copy.
    pub copy_bytes: u64,
    /// Bytes for the large read and write.
    pub large_bytes: u64,
    /// Scenario names to run (all when empty).
    pub only: Vec<String>,
    /// Leave out the sign-ons that fail (a wrong password counts towards
    /// `QMAXSIGN`).
    pub no_bad_signon: bool,
}

/// The usage line.
pub const USAGE: &str = "smbtest HOST PORT USER PASSWORD SHARE SCRATCH \
[--only A,B] [--copy-bytes N] [--large-bytes N] [--print-share NAME] [--no-bad-signon] [--list]\n\
PASSWORD `-` reads SMBTEST_PASSWORD from the environment.";

impl Target {
    /// One with the defaults for everything but the six positional values.
    pub fn new(
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        share: &str,
        scratch: &str,
    ) -> Target {
        Target {
            host: host.to_string(),
            port,
            user: user.to_string(),
            password: password.to_string(),
            share: share.to_string(),
            scratch: scratch.trim_matches(['\\', '/']).replace('/', "\\"),
            print_share: None,
            copy_bytes: 64 << 20,
            large_bytes: 8 << 20,
            only: Vec::new(),
            no_bad_signon: false,
        }
    }

    /// From the arguments after the program name. `Ok(None)` for `--list`.
    pub fn from_args(args: &[String]) -> Result<Option<Target>, Error> {
        let mut pos = Vec::new();
        let mut opts = Target::new("", 0, "", "", "", "");
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut value = |name: &str| {
                it.next()
                    .cloned()
                    .ok_or_else(|| Error::Usage(format!("{name} needs a value")))
            };
            match a.as_str() {
                "--list" => return Ok(None),
                "--only" => {
                    opts.only = value("--only")?.split(',').map(str::to_string).collect();
                }
                "--copy-bytes" => opts.copy_bytes = bytes(&value("--copy-bytes")?)?,
                "--large-bytes" => opts.large_bytes = bytes(&value("--large-bytes")?)?,
                "--print-share" => opts.print_share = Some(value("--print-share")?),
                "--no-bad-signon" => opts.no_bad_signon = true,
                s if s.starts_with("--") => {
                    return Err(Error::Usage(format!("unknown option {s}")));
                }
                s => pos.push(s.to_string()),
            }
        }
        let [host, port, user, password, share, scratch] = pos.as_slice() else {
            return Err(Error::Usage(USAGE.to_string()));
        };
        let port = port
            .parse()
            .map_err(|_| Error::Usage(format!("bad port {port}")))?;
        let password = if password == "-" {
            std::env::var("SMBTEST_PASSWORD")
                .map_err(|_| Error::Usage("SMBTEST_PASSWORD is not set".into()))?
        } else {
            password.clone()
        };
        let base = Target::new(host, port, user, &password, share, scratch);
        Ok(Some(Target {
            only: opts.only,
            print_share: opts.print_share,
            no_bad_signon: opts.no_bad_signon,
            copy_bytes: opts.copy_bytes,
            large_bytes: opts.large_bytes,
            ..base
        }))
    }

    /// Whether `name` is selected.
    pub fn wants(&self, name: &str) -> bool {
        self.only.is_empty() || self.only.iter().any(|o| o == name)
    }
}

/// `N`, `NK`, `NM` or `NG` (binary multiples).
fn bytes(s: &str) -> Result<u64, Error> {
    let (num, shift) = match s.as_bytes().last() {
        Some(b'K' | b'k') => (&s[..s.len() - 1], 10),
        Some(b'M' | b'm') => (&s[..s.len() - 1], 20),
        Some(b'G' | b'g') => (&s[..s.len() - 1], 30),
        _ => (s, 0),
    };
    num.parse::<u64>()
        .map(|n| n << shift)
        .map_err(|_| Error::Usage(format!("bad byte count {s}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn parses_positionals_and_options() {
        let t = Target::from_args(&args(
            "h 4445 u p s /a/b/ --only x,y --copy-bytes 1G --no-bad-signon",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            (t.host.as_str(), t.port, t.scratch.as_str()),
            ("h", 4445, "a\\b")
        );
        assert_eq!(t.only, ["x", "y"]);
        assert_eq!(t.copy_bytes, 1 << 30);
        assert!(t.no_bad_signon && t.wants("x") && !t.wants("z"));
    }

    #[test]
    fn refuses_short_lines() {
        assert!(Target::from_args(&args("h 445 u")).is_err());
        assert!(Target::from_args(&args("--list")).unwrap().is_none());
    }
}
