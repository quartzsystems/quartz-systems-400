// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage service's answers on the wire: snapshots, the
//! resolver's configuration, port restrictions, services.

use std::net::IpAddr;

use ipc::{Dec, Wire, WireError, wire_enum, wire_struct};

use crate::model::Proto;
use crate::model::port::PortRestriction;
use crate::model::tables::{ServiceEntry, ServicesTable};
use crate::msg::Note;
use crate::resolver::HostConfig;
use crate::state::{IfcView, RteView, Snapshot};

wire_enum!(Proto { 0 => Tcp, 1 => Udp });
wire_struct!(PortRestriction { lower, upper, proto, user });
wire_struct!(ServiceEntry { name, port, proto, aliases, text });
wire_struct!(ServicesTable { list });
wire_struct!(Note { msgid, text, error });
wire_struct!(IfcView {
    addr, mask, prefix, line, vlan, line_type, status, alias, local_ifc, tos, mtu, autostart, text, link, error
});
wire_struct!(RteView { dest, mask, next_hop, bind_ifc, tos, mtu, dup_pty, metric, redist, text, status, link });
wire_struct!(Snapshot { tcp, ifcs, rtes });

fn ip_text(ip: &IpAddr) -> String {
    ip.to_string()
}

fn ip_of(s: &str) -> Result<IpAddr, WireError> {
    s.parse().map_err(|_| WireError::Utf8)
}

impl Wire for HostConfig {
    fn put(&self, out: &mut Vec<u8>) {
        let hosts: Vec<(String, Vec<String>)> = self.hosts.iter().map(|(a, n)| (ip_text(a), n.clone())).collect();
        let dns: Vec<String> = self.dns.iter().map(ip_text).collect();
        hosts.put(out);
        self.host_name.put(out);
        self.dmn_name.put(out);
        self.dmn_sch_list.put(out);
        self.host_sch_pty_local.put(out);
        dns.put(out);
        self.port.put(out);
        self.tcp.put(out);
        self.init_qry_int.put(out);
        self.qry_timeout.put(out);
    }

    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let hosts: Vec<(String, Vec<String>)> = Wire::get(d)?;
        let host_name = Wire::get(d)?;
        let dmn_name = Wire::get(d)?;
        let dmn_sch_list = Wire::get(d)?;
        let host_sch_pty_local = Wire::get(d)?;
        let dns: Vec<String> = Wire::get(d)?;
        Ok(Self {
            hosts: hosts
                .into_iter()
                .map(|(a, n)| ip_of(&a).map(|a| (a, n)))
                .collect::<Result<_, _>>()?,
            host_name,
            dmn_name,
            dmn_sch_list,
            host_sch_pty_local,
            dns: dns.iter().map(|s| ip_of(s)).collect::<Result<_, _>>()?,
            port: Wire::get(d)?,
            tcp: Wire::get(d)?,
            init_qry_int: Wire::get(d)?,
            qry_timeout: Wire::get(d)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::state::State;

    #[test]
    fn round_trips() {
        let cfg = Config::default();
        let snap = State::default().snapshot(&cfg);
        assert_eq!(ipc::decode::<Snapshot>(&ipc::encode(&snap)).unwrap(), snap);
        let h = HostConfig::from_config(&cfg);
        assert_eq!(ipc::decode::<HostConfig>(&ipc::encode(&h)).unwrap(), h);
        let s = cfg.services.clone();
        assert_eq!(ipc::decode::<ServicesTable>(&ipc::encode(&s)).unwrap(), s);
    }
}
