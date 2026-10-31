// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the hardware resource service answers, over its socket: the
//! table (with the trees in each resource's parents), one resource, and
//! a rescan on demand.

use ipc::{Dec, Wire, WireError, wire_enum, wire_struct};

use crate::scan::nic::{Family, NetPort, family};
use crate::table::{Kind, Resource, Status, Table};

wire_enum!(Kind {
    0 => Bridge,
    1 => Lan,
    2 => Storage,
    3 => Disk,
    4 => Optical,
    5 => Comm,
    6 => Port,
    7 => Keyboard,
    8 => Other,
    9 => Tape,
    10 => TapeLibrary,
    11 => Processor,
    12 => Memory,
});

wire_enum!(Status {
    0 => Operational,
    1 => NotDetected,
});

/// A family travels as its driver's name.
impl Wire for Family {
    fn put(&self, out: &mut Vec<u8>) {
        self.driver.to_string().put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(family(&String::get(d)?))
    }
}

wire_struct!(NetPort {
    ifname,
    family,
    driver,
    pci,
    ids,
    dev_port,
    mac,
    carrier,
    speed,
    devpath,
});

wire_struct!(Resource {
    name,
    kind,
    location,
    description,
    devpath,
    status,
    type_model,
    port,
    serial,
    parent,
    logical,
});

wire_struct!(Table { resources });

ipc::rpc! {
    request Request, service Service, client Client, dispatch dispatch;
    /// The whole table; with `refresh`, the ports' link state read again.
    0 => fn table / Table (refresh: bool) -> Table;
    /// One resource by name.
    1 => fn resource / Resource (name: String) -> Option<Resource>;
    /// Walk sysfs again now; the changes, one line each.
    2 => fn rescan / Rescan () -> Vec<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_round_trips() {
        let mut r = Resource::new("CMN01".into(), Kind::Port, "0000:01:00.0".into(), "port".into(), "/x".into());
        r.port = Some(NetPort {
            ifname: "eth0".into(),
            family: family("virtio_net"),
            driver: "virtio_net".into(),
            pci: "0000:01:00.0".into(),
            ids: "1af4:1041".into(),
            dev_port: 0,
            mac: "52:54:00:12:34:56".into(),
            carrier: Some(true),
            speed: None,
            devpath: "/x/net/eth0".into(),
        });
        r.type_model = Some(("QN01".into(), "001".into()));
        let t = Table { resources: vec![r] };
        let back: Table = ipc::decode(&ipc::encode(&t)).unwrap();
        assert_eq!(back, t);
        assert_eq!(back.resources[0].port.as_ref().unwrap().family.type_code, "QN01");
    }
}
