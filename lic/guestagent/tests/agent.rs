// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The agent as a hypervisor's management stack talks to it: the
//! commands libvirt and Proxmox send, in the QEMU guest agent's wire
//! form, over a guest in memory.

use guestagent::Agent;
use guestagent::facts::{Address, Filesystem, Guest, Interface, Kernel, Stop};
use guestagent::status::Status;
use serde_json::Value;

#[derive(Default)]
struct Mem {
    stopped: Vec<Stop>,
    refuse: bool,
    power: Vec<guestagent::ups::Power>,
}

impl Guest for Mem {
    fn status(&mut self) -> Status {
        Status {
            system: "QSHQ400".into(),
            release: "V1R1M0".into(),
            state: "complete".into(),
            mode: "normal".into(),
            ipl_type: "B".into(),
            slot: "B".into(),
            src: "C9002A00".into(),
            src_text: "IPL complete".into(),
            ipl_time: 1_793_000_000,
        }
    }
    fn kernel(&mut self) -> Kernel {
        Kernel {
            release: "6.18.2".into(),
            version: "#1 SMP".into(),
            machine: "x86_64".into(),
        }
    }
    fn interfaces(&mut self) -> Vec<Interface> {
        vec![
            Interface {
                name: "lo".into(),
                mac: "00:00:00:00:00:00".into(),
                addresses: vec![Address {
                    family: "ipv4",
                    address: "127.0.0.1".into(),
                    prefix: 8,
                }],
                stats: [1, 2, 0, 0, 1, 2, 0, 0],
            },
            Interface {
                name: "eth0".into(),
                mac: "52:54:00:12:34:56".into(),
                addresses: vec![
                    Address {
                        family: "ipv4",
                        address: "10.0.2.15".into(),
                        prefix: 24,
                    },
                    Address {
                        family: "ipv6",
                        address: "fe80::5054:ff:fe12:3456".into(),
                        prefix: 64,
                    },
                ],
                stats: [100, 10, 0, 1, 200, 20, 0, 0],
            },
        ]
    }
    fn filesystems(&mut self) -> Vec<Filesystem> {
        vec![Filesystem {
            name: "IFS".into(),
            mountpoint: "/".into(),
            kind: "xfs".into(),
            used: 1 << 30,
            total: 8 << 30,
        }]
    }
    fn time_ns(&mut self) -> i64 {
        1_793_000_000_000_000_000
    }
    fn cpus(&mut self) -> usize {
        2
    }
    fn stop(&mut self, how: Stop) -> Result<(), String> {
        if self.refuse {
            return Err("The system cannot power down now (state ipl)".into());
        }
        self.stopped.push(how);
        Ok(())
    }
    fn set_power(&mut self, power: guestagent::ups::Power) -> Result<(), String> {
        self.power.push(power);
        Ok(())
    }
}

/// The JSON values of a reply stream, one per line.
fn replies(out: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(out)
        .lines()
        .map(|l| serde_json::from_str(l.trim_start_matches('\u{FFFD}')).unwrap())
        .collect()
}

fn one(a: &mut Agent<Mem>, req: &str) -> Value {
    let r = replies(&a.feed(req.as_bytes()));
    assert_eq!(r.len(), 1, "{r:?}");
    r.into_iter().next().unwrap()
}

#[test]
fn sync_and_ping() {
    let mut a = Agent::new(Mem::default());
    let r = one(
        &mut a,
        r#"{"execute":"guest-sync","arguments":{"id":123456}}"#,
    );
    assert_eq!(r["return"], 123_456);
    assert_eq!(
        one(&mut a, r#"{"execute":"guest-ping"}"#),
        serde_json::json!({ "return": {} })
    );
    // The delimited sync answers after a 0xFF, which the host waits for.
    let out = a.feed(b"\xff{\"execute\":\"guest-sync-delimited\",\"arguments\":{\"id\":7}}");
    assert_eq!(out[0], 0xFF);
    let v: Value = serde_json::from_slice(&out[1..]).unwrap();
    assert_eq!(v["return"], 7);
    let e = one(&mut a, r#"{"execute":"guest-sync"}"#);
    assert_eq!(e["error"]["class"], "GenericError");
}

#[test]
fn info_lists_every_command() {
    let mut a = Agent::new(Mem::default());
    let r = one(&mut a, r#"{"execute":"guest-info"}"#);
    let cmds = r["return"]["supported_commands"].as_array().unwrap();
    let names: Vec<&str> = cmds.iter().map(|c| c["name"].as_str().unwrap()).collect();
    for want in [
        "guest-ping",
        "guest-sync-delimited",
        "guest-shutdown",
        "guest-network-get-interfaces",
        "guest-get-fsinfo",
        "guest-get-osinfo",
    ] {
        assert!(names.contains(&want), "{want}");
    }
    let sd = cmds.iter().find(|c| c["name"] == "guest-shutdown").unwrap();
    assert_eq!(sd["success-response"], false);
    assert!(r["return"]["version"].as_str().unwrap().contains("qs400"));
}

#[test]
fn facts() {
    let mut a = Agent::new(Mem::default());
    assert_eq!(
        one(&mut a, r#"{"execute":"guest-get-host-name"}"#)["return"]["host-name"],
        "QSHQ400"
    );
    let os = one(&mut a, r#"{"execute":"guest-get-osinfo"}"#)["return"].clone();
    assert_eq!(
        (os["id"].as_str(), os["pretty-name"].as_str()),
        (Some("qs400"), Some("QS/400 V1R1M0"))
    );
    assert_eq!(os["kernel-release"], "6.18.2");
    let n = one(&mut a, r#"{"execute":"guest-network-get-interfaces"}"#)["return"].clone();
    assert_eq!(n[1]["name"], "eth0");
    assert_eq!(n[1]["hardware-address"], "52:54:00:12:34:56");
    assert_eq!(n[1]["ip-addresses"][0]["ip-address"], "10.0.2.15");
    assert_eq!(n[1]["ip-addresses"][1]["ip-address-type"], "ipv6");
    assert_eq!(n[1]["statistics"]["tx-bytes"], 200);
    let fs = one(&mut a, r#"{"execute":"guest-get-fsinfo"}"#)["return"].clone();
    assert_eq!(fs[0]["total-bytes"], 8u64 << 30);
    assert_eq!(fs[0]["used-bytes"], 1u64 << 30);
    let st = one(&mut a, r#"{"execute":"qs400-get-status","id":"x1"}"#);
    assert_eq!(st["id"], "x1");
    assert_eq!(st["return"]["last-src"], "C9002A00");
    assert_eq!(st["return"]["ipl-state"], "complete");
    assert_eq!(
        one(&mut a, r#"{"execute":"guest-get-vcpus"}"#)["return"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        one(&mut a, r#"{"execute":"guest-fsfreeze-status"}"#)["return"],
        "thawed"
    );
}

#[test]
fn shutdown_is_a_controlled_power_down() {
    let mut a = Agent::new(Mem::default());
    // Success sends nothing, as the QEMU agent.
    assert!(
        a.feed(br#"{"execute":"guest-shutdown","arguments":{"mode":"reboot"}}"#)
            .is_empty()
    );
    assert_eq!(a.guest().stopped, [Stop::Restart]);
    let again = one(&mut a, r#"{"execute":"guest-shutdown"}"#);
    assert_eq!(again["error"]["class"], "GenericError");
    let mut b = Agent::new(Mem::default());
    assert!(b.feed(br#"{"execute":"guest-shutdown"}"#).is_empty());
    assert_eq!(b.guest().stopped, [Stop::PowerDown]);
    let bad = one(
        &mut b,
        r#"{"execute":"guest-shutdown","arguments":{"mode":"sleep"}}"#,
    );
    assert!(bad["error"]["desc"].as_str().unwrap().contains("sleep"));
    let mut c = Agent::new(Mem {
        refuse: true,
        ..Mem::default()
    });
    let no = one(&mut c, r#"{"execute":"guest-shutdown"}"#);
    assert!(
        no["error"]["desc"]
            .as_str()
            .unwrap()
            .contains("cannot power down")
    );
    assert!(c.stopping.is_none());
}

#[test]
fn errors() {
    let mut a = Agent::new(Mem::default());
    let r = one(
        &mut a,
        r#"{"execute":"guest-exec","arguments":{"path":"/bin/sh"},"id":5}"#,
    );
    assert_eq!(r["error"]["class"], "CommandNotFound");
    assert_eq!(r["id"], 5);
    assert_eq!(
        one(&mut a, r#"{"nothing":1}"#)["error"]["class"],
        "GenericError"
    );
    assert_eq!(one(&mut a, "[1,2]")["error"]["class"], "GenericError");
    // Requests split across reads, two in one read.
    assert!(a.feed(br#"{"execute":"guest-pi"#).is_empty());
    assert_eq!(
        replies(&a.feed(b"ng\"}{\"execute\":\"guest-ping\"}")).len(),
        2
    );
}

#[test]
fn power_source() {
    let mut a = Agent::new(Mem::default());
    let r = one(
        &mut a,
        r#"{"execute":"qs400-set-power-source","arguments":{"source":"battery"}}"#,
    );
    assert_eq!(r["return"], serde_json::json!({}));
    one(
        &mut a,
        r#"{"execute":"qs400-set-power-source","arguments":{"source":"battery","battery-low":true}}"#,
    );
    one(
        &mut a,
        r#"{"execute":"qs400-set-power-source","arguments":{"source":"utility"}}"#,
    );
    let e = one(
        &mut a,
        r#"{"execute":"qs400-set-power-source","arguments":{"source":"solar"}}"#,
    );
    assert_eq!(e["error"]["class"], "GenericError");
    let p = |on_battery, battery_low| guestagent::ups::Power {
        on_battery,
        battery_low,
    };
    assert_eq!(
        a.guest().power,
        vec![p(true, false), p(true, true), p(false, false)]
    );
}
