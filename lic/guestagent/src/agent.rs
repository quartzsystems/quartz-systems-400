// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The commands the agent answers, as the QEMU guest agent answers
//! them, so a hypervisor's management stack needs nothing special:
//!
//! - `guest-sync`, `guest-sync-delimited` (its reply after a `0xFF`),
//!   `guest-ping`, `guest-info` (the commands, every one enabled);
//! - `guest-get-host-name` (the system name), `guest-get-osinfo` (QS/400
//!   and its release, the kernel underneath), `guest-get-time`,
//!   `guest-get-timezone`, `guest-get-vcpus`, `guest-get-users` (none);
//! - `guest-network-get-interfaces` (each interface's MAC, addresses and
//!   counters), `guest-get-fsinfo` (the ASP and the IFS: used and total
//!   bytes), `guest-fsfreeze-status` (`thawed`: QS/400 does not freeze);
//! - `guest-shutdown`: `powerdown` and `halt` are `PWRDWNSYS
//!   OPTION(*CNTRLD)`, `reboot` adds `RESTART(*YES)`; as the QEMU agent,
//!   a shutdown that starts sends no reply;
//! - `qs400-get-status`: the release, IPL state, mode, type and the last
//!   SRC with its text.
//!
//! Anything else is `CommandNotFound`; a request that is not JSON or has
//! no `execute` is a `GenericError`. Replies carry the request's `id`
//! when it has one.

use serde_json::{Map, Value, json};

use crate::facts::{Guest, Stop};
use crate::framer::{Framer, Piece};

/// Every command, as `guest-info` lists them.
pub const COMMANDS: &[&str] = &[
    "guest-sync",
    "guest-sync-delimited",
    "guest-ping",
    "guest-info",
    "guest-get-host-name",
    "guest-get-osinfo",
    "guest-get-time",
    "guest-get-timezone",
    "guest-get-vcpus",
    "guest-get-users",
    "guest-network-get-interfaces",
    "guest-get-fsinfo",
    "guest-fsfreeze-status",
    "guest-shutdown",
    "qs400-get-status",
];

/// Commands that send no reply when they succeed.
const NO_SUCCESS_REPLY: &[&str] = &["guest-shutdown"];

/// The agent over a guest.
pub struct Agent<G: Guest> {
    guest: G,
    framer: Framer,
    /// Set once a shutdown was accepted.
    pub stopping: Option<Stop>,
}

fn error(class: &str, desc: &str) -> Value {
    json!({ "error": { "class": class, "desc": desc } })
}

/// A reply's bytes: the JSON and a newline, after a `0xFF` for
/// `guest-sync-delimited`.
fn bytes(v: &Value, delimited: bool) -> Vec<u8> {
    let mut b = Vec::new();
    if delimited {
        b.push(0xFF);
    }
    b.extend(v.to_string().into_bytes());
    b.push(b'\n');
    b
}

impl<G: Guest> Agent<G> {
    /// An agent over `guest`.
    pub fn new(guest: G) -> Self {
        Self {
            guest,
            framer: Framer::new(),
            stopping: None,
        }
    }

    /// The guest.
    pub fn guest(&mut self) -> &mut G {
        &mut self.guest
    }

    /// Bytes from the host; the bytes to send back.
    pub fn feed(&mut self, input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for p in self.framer.push(input) {
            match p {
                Piece::Object(text) => out.extend(self.request(&text)),
                Piece::Junk => out.extend(bytes(&error("GenericError", "Invalid JSON syntax"), false)),
            }
        }
        out
    }

    /// One request's reply bytes; empty when it sends none.
    pub fn request(&mut self, text: &str) -> Vec<u8> {
        let Ok(Value::Object(req)) = serde_json::from_str::<Value>(text) else {
            return bytes(&error("GenericError", "Invalid JSON syntax"), false);
        };
        let id = req.get("id").cloned();
        let Some(cmd) = req.get("execute").and_then(Value::as_str) else {
            return bytes(&error("GenericError", "Expecting the key 'execute'"), false);
        };
        let empty = Map::new();
        let args = req.get("arguments").and_then(Value::as_object).unwrap_or(&empty);
        let delimited = cmd == "guest-sync-delimited";
        let mut v = match self.run(cmd, args) {
            Ok(None) => return Vec::new(),
            Ok(Some(r)) => json!({ "return": r }),
            Err(e) => e,
        };
        if let (Some(id), Some(o)) = (id, v.as_object_mut()) {
            o.insert("id".into(), id);
        }
        bytes(&v, delimited)
    }

    /// A command's result; `None` for no reply.
    fn run(&mut self, cmd: &str, args: &Map<String, Value>) -> Result<Option<Value>, Value> {
        let r = match cmd {
            "guest-sync" | "guest-sync-delimited" => match args.get("id") {
                Some(id) if id.is_number() => id.clone(),
                _ => return Err(error("GenericError", "Parameter 'id' is missing")),
            },
            "guest-ping" => json!({}),
            "guest-info" => {
                let cmds: Vec<Value> = COMMANDS
                    .iter()
                    .map(|c| json!({ "name": c, "enabled": true, "success-response": !NO_SUCCESS_REPLY.contains(c) }))
                    .collect();
                json!({ "version": crate::version(), "supported_commands": cmds })
            }
            "guest-get-host-name" => json!({ "host-name": self.guest.status().system }),
            "guest-get-osinfo" => self.osinfo(),
            "guest-get-time" => json!(self.guest.time_ns()),
            "guest-get-timezone" => json!({ "zone": "UTC", "offset": 0 }),
            "guest-get-vcpus" => {
                let v: Vec<Value> = (0..self.guest.cpus())
                    .map(|i| json!({ "logical-id": i, "online": true, "can-offline": false }))
                    .collect();
                json!(v)
            }
            "guest-get-users" => json!([]),
            "guest-network-get-interfaces" => self.interfaces(),
            "guest-get-fsinfo" => self.fsinfo(),
            "guest-fsfreeze-status" => json!("thawed"),
            "guest-shutdown" => return self.shutdown(args),
            "qs400-get-status" => {
                let s = self.guest.status();
                json!({
                    "system-name": s.system,
                    "release": s.release,
                    "ipl-state": s.state,
                    "mode": s.mode,
                    "ipl-type": s.ipl_type,
                    "slot": s.slot,
                    "last-src": s.src,
                    "src-text": s.src_text,
                    "ipl-time": s.ipl_time,
                })
            }
            _ => return Err(error("CommandNotFound", &format!("The command {cmd} has not been found"))),
        };
        Ok(Some(r))
    }

    fn osinfo(&mut self) -> Value {
        let s = self.guest.status();
        let k = self.guest.kernel();
        let rel = if s.release.is_empty() { release::RELEASE.to_string() } else { s.release };
        let r = release::RELEASE;
        json!({
            "id": "qs400",
            "name": "QS/400",
            "pretty-name": format!("QS/400 {rel}"),
            "version": rel,
            "version-id": format!("{}.{}.{}", r.v, r.r, r.m),
            "kernel-release": k.release,
            "kernel-version": k.version,
            "machine": k.machine,
        })
    }

    fn interfaces(&mut self) -> Value {
        let v: Vec<Value> = self
            .guest
            .interfaces()
            .into_iter()
            .map(|i| {
                let addrs: Vec<Value> = i
                    .addresses
                    .iter()
                    .map(|a| json!({ "ip-address-type": a.family, "ip-address": a.address, "prefix": a.prefix }))
                    .collect();
                let [rb, rp, re, rd, tb, tp, te, td] = i.stats;
                let mut o = json!({
                    "name": i.name,
                    "ip-addresses": addrs,
                    "statistics": {
                        "rx-bytes": rb, "rx-packets": rp, "rx-errs": re, "rx-dropped": rd,
                        "tx-bytes": tb, "tx-packets": tp, "tx-errs": te, "tx-dropped": td,
                    },
                });
                if !i.mac.is_empty() {
                    o["hardware-address"] = json!(i.mac);
                }
                o
            })
            .collect();
        json!(v)
    }

    fn fsinfo(&mut self) -> Value {
        let v: Vec<Value> = self
            .guest
            .filesystems()
            .into_iter()
            .map(|f| {
                json!({
                    "name": f.name,
                    "mountpoint": f.mountpoint,
                    "type": f.kind,
                    "used-bytes": f.used,
                    "total-bytes": f.total,
                    "disk": [],
                })
            })
            .collect();
        json!(v)
    }

    fn shutdown(&mut self, args: &Map<String, Value>) -> Result<Option<Value>, Value> {
        let how = match args.get("mode").and_then(Value::as_str).unwrap_or("powerdown") {
            "powerdown" | "halt" => Stop::PowerDown,
            "reboot" => Stop::Restart,
            m => return Err(error("GenericError", &format!("Parameter 'mode' does not accept value '{m}'"))),
        };
        if self.stopping.is_some() {
            return Err(error("GenericError", "A power down is already in progress"));
        }
        match self.guest.stop(how) {
            Ok(()) => {
                self.stopping = Some(how);
                Ok(None)
            }
            Err(e) => Err(error("GenericError", &e)),
        }
    }
}
