// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the scenarios run against, from the command line.

use std::time::Duration;

use crate::Error;
use crate::rpc::Proto;

/// The NFS version the scenarios use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vers {
    /// Version 3 with MOUNT, NLM and NSM.
    V3,
    /// Version 4, minor version 0.
    V40,
    /// Version 4, minor version 1.
    V41,
}

/// What `nfstest` was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Run the scenarios.
    Run,
    /// Print the binder's registrations and the export list.
    Probe,
    /// Sequential write then read of `mib` MiB in blocks of `bs` bytes
    /// (0: the server's preferred size).
    Bench {
        /// MiB to move.
        mib: u64,
        /// Block size.
        bs: u32,
    },
    /// Print the scenario names.
    List,
}

/// The server, the export and the client's choices.
#[derive(Clone, Debug)]
pub struct Target {
    /// What to do.
    pub command: Command,
    /// Host name or address.
    pub host: String,
    /// The export's path on the server (for v4, its path in the pseudo file system).
    pub export: String,
    /// A directory under the export the scenarios write in (made when missing).
    pub scratch: String,
    /// The version.
    pub vers: Vers,
    /// The transport.
    pub proto: Proto,
    /// The binder's port (111).
    pub pmap_port: u16,
    /// NFS's port; when not given, the binder's answer (or 2049).
    pub port: Option<u16>,
    /// MOUNT's port.
    pub mount_port: Option<u16>,
    /// NLM's port.
    pub nlm_port: Option<u16>,
    /// NSM's port.
    pub nsm_port: Option<u16>,
    /// Guest port to host port, applied to the binder's answers.
    pub port_map: Vec<(u16, u16)>,
    /// `AUTH_SYS` UID.
    pub uid: u32,
    /// `AUTH_SYS` GID.
    pub gid: u32,
    /// UID 0 is not squashed by this export.
    pub expect_root: bool,
    /// Run the scenarios that need the server to call this client back.
    pub callbacks: bool,
    /// The address the callback listeners bind (and the server reaches).
    pub cb_host: String,
    /// The callback portmapper's port (the server asks 111).
    pub cb_pmap_port: u16,
    /// The callback lock manager's port (0: any).
    pub cb_nlm_port: u16,
    /// The server has just started: its grace period is on.
    pub expect_grace: bool,
    /// The wait for one reply.
    pub timeout: Duration,
    /// MiB of the large file.
    pub large_mib: u64,
    /// Entries of the large directory.
    pub entries: u32,
    /// Scenario names to run (all when empty).
    pub only: Vec<String>,
}

/// The usage text.
pub const USAGE: &str = "\
nfstest HOST EXPORT SCRATCH [options]          run the scenarios
nfstest probe HOST [--pmap-port N] [--port-map G=H ...] [EXPORT]
                                               the binder's table and the export list
nfstest bench HOST EXPORT SCRATCH [--mib N] [--bs N] [options]
                                               sequential write then read, MiB/s
nfstest --list                                 the scenario names
options: --vers 3|4.0|4.1  --proto tcp|udp  --pmap-port N  --port N  --mountport N
  --nlmport N  --nsmport N  --port-map GUEST=HOST (repeatable; GUEST:HOST too)
  --uid N  --gid N  --expect-root  --callbacks [--cb-host IP] [--cb-pmap-port N]
  [--cb-nlm-port N]  --expect-grace  --timeout SECS  --large-mib N  --entries N
  --only A,B
exit code: the number of failed scenarios (at most 100); 101 for a usage error";

impl Target {
    /// The defaults with these three.
    pub fn new(host: &str, export: &str, scratch: &str) -> Target {
        Target {
            command: Command::Run,
            host: host.to_string(),
            export: export.to_string(),
            scratch: scratch.trim_matches('/').to_string(),
            vers: Vers::V3,
            proto: Proto::Tcp,
            pmap_port: 111,
            port: None,
            mount_port: None,
            nlm_port: None,
            nsm_port: None,
            port_map: Vec::new(),
            uid: 0,
            gid: 0,
            expect_root: false,
            callbacks: false,
            cb_host: "127.0.0.1".into(),
            cb_pmap_port: 111,
            cb_nlm_port: 0,
            expect_grace: false,
            timeout: Duration::from_secs(5),
            large_mib: 16,
            entries: 1500,
            only: Vec::new(),
        }
    }

    /// From the arguments after the program name.
    #[allow(clippy::too_many_lines)]
    pub fn from_args(args: &[String]) -> Result<Target, Error> {
        let mut it = args.iter().peekable();
        let mut t = Target::new("", "", "");
        let mut mib = 1024;
        let mut bs = 0;
        match it.peek().map(|s| s.as_str()) {
            Some("probe") => {
                t.command = Command::Probe;
                it.next();
            }
            Some("bench") => {
                t.command = Command::Bench { mib: 0, bs: 0 };
                it.next();
            }
            _ => {}
        }
        let mut pos = Vec::new();
        while let Some(a) = it.next() {
            let mut value = |name: &str| {
                it.next()
                    .cloned()
                    .ok_or_else(|| Error::Usage(format!("{name} needs a value")))
            };
            match a.as_str() {
                "--list" => t.command = Command::List,
                "--vers" => {
                    t.vers = match value("--vers")?.as_str() {
                        "3" => Vers::V3,
                        "4" | "4.0" => Vers::V40,
                        "4.1" => Vers::V41,
                        v => return Err(Error::Usage(format!("bad version {v}"))),
                    };
                }
                "--proto" => {
                    t.proto = match value("--proto")?.as_str() {
                        "tcp" => Proto::Tcp,
                        "udp" => Proto::Udp,
                        p => return Err(Error::Usage(format!("bad protocol {p}"))),
                    };
                }
                "--pmap-port" => t.pmap_port = num(&value(a)?)?,
                "--port" => t.port = Some(num(&value(a)?)?),
                "--mountport" => t.mount_port = Some(num(&value(a)?)?),
                "--nlmport" => t.nlm_port = Some(num(&value(a)?)?),
                "--nsmport" => t.nsm_port = Some(num(&value(a)?)?),
                "--port-map" => {
                    let v = value(a)?;
                    let (g, h) = v
                        .split_once(['=', ':'])
                        .ok_or_else(|| Error::Usage(format!("bad port map {v}")))?;
                    t.port_map.push((num(g)?, num(h)?));
                }
                "--uid" => t.uid = num(&value(a)?)?,
                "--gid" => t.gid = num(&value(a)?)?,
                "--expect-root" => t.expect_root = true,
                "--callbacks" => t.callbacks = true,
                "--cb-host" => t.cb_host = value(a)?,
                "--cb-pmap-port" => t.cb_pmap_port = num(&value(a)?)?,
                "--cb-nlm-port" => t.cb_nlm_port = num(&value(a)?)?,
                "--expect-grace" => t.expect_grace = true,
                "--timeout" => t.timeout = Duration::from_secs(num(&value(a)?)?),
                "--large-mib" => t.large_mib = num(&value(a)?)?,
                "--entries" => t.entries = num(&value(a)?)?,
                "--mib" => mib = num(&value(a)?)?,
                "--bs" => bs = num(&value(a)?)?,
                "--only" => {
                    t.only = value(a)?.split(',').map(str::to_string).collect();
                }
                s if s.starts_with("--") => {
                    return Err(Error::Usage(format!("unknown option {s}")));
                }
                s => pos.push(s.to_string()),
            }
        }
        match (&t.command, pos.as_slice()) {
            (Command::List, _) => {}
            (Command::Probe, [host]) => t.host.clone_from(host),
            (Command::Probe, [host, export]) => {
                t.host.clone_from(host);
                t.export.clone_from(export);
            }
            (Command::Run | Command::Bench { .. }, [host, export, scratch]) => {
                let base = Target::new(host, export, scratch);
                t.host = base.host;
                t.export = base.export;
                t.scratch = base.scratch;
                if let Command::Bench { .. } = t.command {
                    t.command = Command::Bench { mib, bs };
                }
            }
            _ => return Err(Error::Usage("wrong number of arguments".into())),
        }
        Ok(t)
    }

    /// Whether `name` is selected.
    pub fn wants(&self, name: &str) -> bool {
        self.only.is_empty() || self.only.iter().any(|o| o == name)
    }

    /// A port the binder answered, through the port map.
    pub fn map_port(&self, guest: u16) -> u16 {
        self.port_map
            .iter()
            .find(|(g, _)| *g == guest)
            .map_or(guest, |(_, h)| *h)
    }
}

fn num<T: std::str::FromStr>(s: &str) -> Result<T, Error> {
    s.parse()
        .map_err(|_| Error::Usage(format!("bad number {s}")))
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
            "h /exp /tmp/ --port-map 2049=12049 --port-map 635:1635 --vers 4.1 --proto udp --only a,b --uid 100",
        ))
        .unwrap();
        assert_eq!(
            (t.host.as_str(), t.export.as_str(), t.scratch.as_str()),
            ("h", "/exp", "tmp")
        );
        assert_eq!(t.map_port(2049), 12049);
        assert_eq!(t.map_port(635), 1635);
        assert_eq!(t.map_port(7), 7);
        assert_eq!((t.vers, t.proto, t.uid), (Vers::V41, Proto::Udp, 100));
        assert!(t.wants("a") && !t.wants("c"));
    }

    #[test]
    fn subcommands() {
        let p = Target::from_args(&args("probe h --pmap-port 1111")).unwrap();
        assert_eq!((p.command, p.pmap_port), (Command::Probe, 1111));
        let b = Target::from_args(&args("bench h /e s --mib 1024 --bs 65536")).unwrap();
        assert_eq!(
            b.command,
            Command::Bench {
                mib: 1024,
                bs: 65536
            }
        );
        assert_eq!(
            Target::from_args(&args("--list")).unwrap().command,
            Command::List
        );
        assert!(Target::from_args(&args("h /e")).is_err());
    }
}
