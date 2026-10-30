// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Starting and ending servers. `STRTCP STRSVR(*YES)` starts the
//! autostart servers and `ENDTCP ENDSVR(*YES)` ends them through a
//! [`ServerStarter`], which the TCP servers framework implements; until
//! it does, the job's starter runs `*TELNET` and refuses the rest.

/// Starts and ends the servers.
pub trait ServerStarter {
    /// Start a server (`*TELNET`, `*FTP`, a user-defined `*MYSVR`); the
    /// completion text, or why it did not start.
    fn start(&mut self, server: &str) -> Result<String, String>;
    /// End a server; `immediate` as `ENDTCP OPTION(*IMMED)`.
    fn end(&mut self, server: &str, immediate: bool) -> Result<String, String>;
    /// Whether a server is running.
    fn active(&mut self, server: &str) -> bool;
}

/// What starting the autostart servers did: one line per server.
pub fn start_all(starter: &mut dyn ServerStarter, servers: &[String]) -> Vec<(String, Result<String, String>)> {
    servers
        .iter()
        .map(|s| {
            let r = if starter.active(s) {
                Ok(String::new())
            } else {
                starter.start(s)
            };
            (s.clone(), r)
        })
        .collect()
}

/// End every running server of `servers`.
pub fn end_all(
    starter: &mut dyn ServerStarter,
    servers: &[String],
    immediate: bool,
) -> Vec<(String, Result<String, String>)> {
    let mut out = Vec::new();
    for s in servers {
        if starter.active(s) {
            out.push((s.clone(), starter.end(s, immediate)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        running: Vec<String>,
    }

    impl ServerStarter for Fake {
        fn start(&mut self, server: &str) -> Result<String, String> {
            if server == "*FTP" {
                return Err("not in this release".into());
            }
            self.running.push(server.into());
            Ok(format!("{server} starting"))
        }
        fn end(&mut self, server: &str, _: bool) -> Result<String, String> {
            self.running.retain(|s| s != server);
            Ok(format!("{server} ending"))
        }
        fn active(&mut self, server: &str) -> bool {
            self.running.iter().any(|s| s == server)
        }
    }

    #[test]
    fn start_then_end() {
        let mut f = Fake::default();
        let list = vec!["*TELNET".to_string(), "*FTP".to_string()];
        let r = start_all(&mut f, &list);
        assert!(r[0].1.is_ok() && r[1].1.is_err());
        assert_eq!(start_all(&mut f, &list)[0].1, Ok(String::new()), "already running");
        let e = end_all(&mut f, &list, false);
        assert_eq!(e.len(), 1);
        assert!(f.running.is_empty());
    }
}
