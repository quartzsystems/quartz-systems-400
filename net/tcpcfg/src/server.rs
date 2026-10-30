// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Starting and ending servers. `STRTCP STRSVR(*YES)` starts the
//! autostart servers and `ENDTCP ENDSVR(*YES)` ends them through a
//! [`ServerStarter`], which the TCP servers framework implements
//! (`servers::framework::Starter`).

use std::time::Duration;

/// How long `ENDTCP` waits for the servers it ended to go.
const END_WAIT: Duration = Duration::from_secs(30);
/// How often it looks.
const END_POLL: Duration = Duration::from_millis(100);

/// Starts and ends the servers.
pub trait ServerStarter {
    /// Start a server (`*TELNET`, `*FTP`, a user-defined `*MYSVR`); the
    /// completion text, or why it did not start.
    fn start(&mut self, server: &str) -> Result<String, String>;
    /// End a server; `immediate` as `ENDTCP OPTION(*IMMED)`.
    fn end(&mut self, server: &str, immediate: bool) -> Result<String, String>;
    /// Whether a server is running.
    fn active(&mut self, server: &str) -> bool;
    /// Wait `d` before looking again whether an ended server is gone.
    fn pause(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
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

/// End every running server of `servers`, then wait (up to
/// [`END_WAIT`]) for their jobs to go: a server job ends on its own
/// time, and one still running would make the next `STRTCP` take it as
/// started and skip it.
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
    let mut waited = Duration::ZERO;
    while waited < END_WAIT && out.iter().any(|(s, r)| r.is_ok() && starter.active(s)) {
        starter.pause(END_POLL);
        waited += END_POLL;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        running: Vec<String>,
        /// Looks an ended server still answers as active.
        ending: Vec<(String, u32)>,
        lag: u32,
        pauses: u32,
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
            if self.lag > 0 {
                self.ending.push((server.into(), self.lag));
            }
            Ok(format!("{server} ending"))
        }
        fn active(&mut self, server: &str) -> bool {
            if let Some(i) = self.ending.iter().position(|(s, _)| s == server) {
                self.ending[i].1 -= 1;
                if self.ending[i].1 == 0 {
                    self.ending.remove(i);
                }
                return true;
            }
            self.running.iter().any(|s| s == server)
        }
        fn pause(&mut self, _: Duration) {
            self.pauses += 1;
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

    #[test]
    fn endtcp_waits_for_ending_servers() {
        // The server job takes a while to go after it is told to end:
        // STRTCP right after ENDTCP must still start it.
        let mut f = Fake {
            lag: 5,
            ..Fake::default()
        };
        let list = vec!["*TELNET".to_string()];
        start_all(&mut f, &list);
        let e = end_all(&mut f, &list, true);
        assert_eq!(e.len(), 1);
        assert!(f.pauses > 0, "ENDTCP returned while the server was still ending");
        let r = start_all(&mut f, &list);
        assert_eq!(r[0].1, Ok("*TELNET starting".to_string()));
    }
}
