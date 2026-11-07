// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NETSTAT_INFO`, `NETSTAT_JOB_INFO`, `NETSTAT_INTERFACE_INFO` and
//! `NETSTAT_ROUTE_INFO`: the lists `NETSTAT` shows, from the same reads
//! of the kernel and TCP/IP's configuration. Socket options and the
//! TCP sequence counters are not read and are null.

use std::net::IpAddr;

use store::txn::Txn;

use super::super::super::SqlContext;
use super::host::Netstat;
use super::{Out, ask};
use crate::error::{Result, SqlError};

/// `NETSTAT_INFO`'s `NOT NULL` columns with no value here.
pub const INFO_NO_VALUE: &[&str] = &["SOCKET_STATE"];

/// `NETSTAT_ROUTE_INFO`'s: a route the kernel has with no interface
/// QS/400 knows has no line.
pub const ROUTE_NO_VALUE: &[&str] = &[
    "LOCAL_BINDING_LINE_DESCRIPTION",
    "LOCAL_BINDING_LINE_TYPE",
    "LOCAL_BINDING_VIRTUAL_LAN_ID",
];

fn lists(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<Netstat> {
    ask(txn, ctx, "the network state", |h| h.netstat())?.map_err(|e| SqlError::Coded {
        code: -443,
        state: "38501",
        what: e,
    })
}

fn kind(a: IpAddr) -> &'static str {
    if a.is_ipv6() { "IPV6" } else { "IPV4" }
}

/// A prefix's mask as an IPv4 dotted quad.
fn mask(prefix: u8) -> String {
    let m: u32 = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - u32::from(prefix.min(32)))
    };
    std::net::Ipv4Addr::from(m).to_string()
}

/// The network of `a` under `prefix`.
fn network(a: IpAddr, prefix: u8) -> IpAddr {
    match a {
        IpAddr::V4(v) => {
            let m: u32 = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - u32::from(prefix.min(32)))
            };
            IpAddr::V4((u32::from(v) & m).into())
        }
        IpAddr::V6(v) => {
            let m: u128 = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - u32::from(prefix.min(128)))
            };
            IpAddr::V6((u128::from(v) & m).into())
        }
    }
}

fn port_name(n: &Netstat, port: u16, tcp: bool) -> Option<&str> {
    n.port_names
        .iter()
        .find(|(p, t, _)| *p == port && *t == tcp)
        .map(|(_, _, s)| s.as_str())
}

fn ends(out: &mut Out, n: &Netstat, c: &nettools::netstat::Conn) {
    out.text("CONNECTION_TYPE", kind(c.local.ip()));
    out.text("REMOTE_ADDRESS", c.remote.ip().to_string());
    out.int("REMOTE_PORT", c.remote.port());
    if c.remote.port() != 0
        && let Some(p) = port_name(n, c.remote.port(), c.tcp)
    {
        out.text("REMOTE_PORT_NAME", p);
    }
    out.text("LOCAL_ADDRESS", c.local.ip().to_string());
    out.int("LOCAL_PORT", c.local.port());
    if let Some(p) = port_name(n, c.local.port(), c.tcp) {
        out.text("LOCAL_PORT_NAME", p);
    }
}

/// `NETSTAT_INFO`.
pub fn info(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let n = lists(txn, ctx)?;
    for c in &n.connections {
        out.row();
        ends(out, &n, c);
        out.text("PROTOCOL", if c.tcp { "TCP" } else { "UDP" });
        if c.tcp {
            out.text("TCP_STATE", c.state.to_ascii_uppercase().replace(' ', "-"));
        }
        out.dec(
            "IDLE_TIME",
            &format!("{}.{:03}", c.idle_ms / 1000, c.idle_ms % 1000),
        );
        out.text("BIND_USER", c.user.as_str());
        out.int("BYTES_SENT_REMOTELY", c.counters.bytes_out);
        out.int("BYTES_RECEIVED_LOCALLY", c.counters.bytes_in);
        out.text("NETWORK_CONNECTION_TYPE", "*TCP");
        if c.tcp {
            out.text(
                "CONNECTION_OPEN_TYPE",
                if c.passive { "PASSIVE" } else { "ACTIVE" },
            );
            out.int("ROUND_TRIP_TIME", c.counters.rtt_us / 1000);
            out.int("ROUND_TRIP_VARIANCE", c.counters.rttvar_us / 1000);
            out.int("CURRENT_RETRANSMISSIONS", c.counters.cur_retrans);
            out.int("TOTAL_RETRANSMISSIONS", c.counters.retrans);
            out.int("OUTGOING_BYTES_BUFFERED", c.counters.out_buffered);
            out.int("INCOMING_BYTES_BUFFERED", c.counters.in_buffered);
            out.int("CONGESTION_WINDOW", c.counters.cwnd);
            out.int("MAXIMUM_SEGMENT_SIZE", c.counters.mss);
        }
        out.int("NUMBER_OF_ASSOCIATED_JOBS", c.jobs.len());
        out.text("CONNECTION_TRANSPORT_LAYER", "TCPIP");
    }
    Ok(())
}

/// `NETSTAT_JOB_INFO`: a row per connection and job holding it.
pub fn jobs(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let n = lists(txn, ctx)?;
    for c in &n.connections {
        for (name, user, number, ty, current) in &c.jobs {
            out.row();
            ends(out, &n, c);
            out.text_or_null("AUTHORIZATION_NAME", current);
            out.text("JOB_NAME", format!("{number}/{user}/{name}"));
            out.text("JOB_NAME_SHORT", name.as_str());
            out.text("JOB_USER", user.as_str());
            out.text("JOB_NUMBER", number.as_str());
            if let Ok(num) = number.parse::<u32>()
                && let Some(id) = wm::job::JobId::new(name, user, num)
            {
                // The start stamp is not in the list: the identifier's
                // number part is right, its stamp zero.
                out.set(
                    "INTERNAL_JOB_ID",
                    crate::value::Value::Bin(id.internal(0).0.to_vec()),
                );
            }
            out.text_or_null("JOB_TYPE_SYSTEM", ty);
            let long = match ty.trim() {
                "BCH" | "BCI" | "PJ" | "EVK" => "BATCH",
                "INT" => "INTERACTIVE",
                "SBS" => "SUBSYSTEM",
                "SYS" => "SYSTEM",
                "ASJ" => "AUTOSTART",
                "WTR" | "PDJ" => "WRITER",
                _ => "",
            };
            out.text_or_null("JOB_TYPE", long);
        }
    }
    Ok(())
}

/// `NETSTAT_INTERFACE_INFO`.
pub fn interfaces(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let n = lists(txn, ctx)?;
    for i in &n.interfaces {
        let Some(a) = i.addr else { continue };
        out.row();
        out.text("CONNECTION_TYPE", kind(a));
        out.text("INTERNET_ADDRESS", a.to_string());
        out.text("NETWORK_ADDRESS", network(a, i.prefix).to_string());
        if a.is_ipv4() {
            out.text("SUBNET_MASK", mask(i.prefix));
            let bits: u32 = u32::MAX >> u32::from(i.prefix.min(31));
            if let IpAddr::V4(v) = a
                && i.prefix < 31
            {
                out.text(
                    "DIRECTED_BROADCAST_ADDRESS",
                    std::net::Ipv4Addr::from(u32::from(v) | bits).to_string(),
                );
            }
        }
        out.int("PREFIX_LENGTH", i.prefix);
        out.text("LINE_DESCRIPTION", i.line.as_str());
        out.text("INTERFACE_LINE_TYPE", i.line_type.trim_start_matches('*'));
        out.text("INTERFACE_STATUS", i.status.to_ascii_uppercase());
        out.text(
            "VIRTUAL_LAN_ID",
            i.vlan.map_or_else(|| "NONE".into(), |v| v.to_string()),
        );
        out.text("MAXIMUM_TRANSMISSION_UNIT", i.mtu.to_string());
        out.text("CONFIGURED_MAXIMUM_TRANSMISSION_UNIT", "LIND");
        out.yes("AUTOSTART", i.autostart);
        out.text(
            "HOST_ADDRESS",
            if a.is_ipv6() {
                "::".to_string()
            } else {
                "0.0.0.0".to_string()
            },
        );
        out.text_or_null("ASSOCIATED_LOCAL_INTERFACE", &i.local_ifc);
        out.text_or_null("SERVICE_TYPE", i.tos.trim_start_matches('*'));
        out.text_or_null("ALIAS_NAME", &i.alias);
        out.text_or_null("INTERFACE_TEXT", &i.text);
        out.yes("DHCP_CREATED", i.dhcp);
    }
    Ok(())
}

/// `NETSTAT_ROUTE_INFO`.
pub fn routes(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let n = lists(txn, ctx)?;
    for r in &n.routes {
        let v6 = r.dest.or(r.hop).is_some_and(|a| a.is_ipv6());
        out.row();
        out.text("CONNECTION_TYPE", if v6 { "IPV6" } else { "IPV4" });
        out.text("ROUTE_DESTINATION", r.dest_text());
        if v6 {
            out.int("PREFIX_LENGTH", r.prefix);
        } else {
            out.text("SUBNET_MASK", mask(r.prefix));
        }
        out.text(
            "NEXT_HOP",
            r.hop.map_or_else(|| "*DIRECT".into(), |h| h.to_string()),
        );
        out.yes("ROUTE_STATUS", r.available);
        out.text("ROUTE_MAXIMUM_TRANSMISSION_UNIT", r.mtu.to_string());
        out.text("ROUTE_TYPE", r.kind().trim_start_matches('*'));
        out.text("ROUTE_SOURCE", if r.configured { "CFG" } else { "SYSTEM" });
        out.text_or_null("SERVICE_TYPE", r.tos.trim_start_matches('*'));
        if let Some(i) = r.ifc {
            out.text("LOCAL_BINDING_INTERFACE", i.to_string());
            if let Some(f) = n.interfaces.iter().find(|f| f.addr == Some(i)) {
                out.text(
                    "LOCAL_BINDING_INTERFACE_STATUS",
                    f.status.to_ascii_uppercase(),
                );
                out.text(
                    "LOCAL_BINDING_NETWORK_ADDRESS",
                    network(i, f.prefix).to_string(),
                );
                if i.is_ipv4() {
                    out.text("LOCAL_BINDING_SUBNET_MASK", mask(f.prefix));
                }
                out.text("LOCAL_BINDING_LINE_DESCRIPTION", f.line.as_str());
                out.text(
                    "LOCAL_BINDING_LINE_TYPE",
                    f.line_type.trim_start_matches('*'),
                );
                out.text(
                    "LOCAL_BINDING_VIRTUAL_LAN_ID",
                    f.vlan.map_or_else(|| "NONE".into(), |v| v.to_string()),
                );
            }
        }
        out.int("ROUTE_PRECEDENCE", r.dup_pty);
        out.text_or_null("ROUTE_TEXT", &r.text);
    }
    Ok(())
}
