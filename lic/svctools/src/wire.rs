// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The process-message encoding of what SST asks the storage service
//! for: the disk facts, the logs and the hardware table (feature
//! `wire`). Privileges and refusals have theirs in `svcusers`.

use ipc::wire_struct;

use crate::env::{DiskUnit, LogEntry, NonConfigured, ParityUnit, ResourceRow};

wire_struct!(DiskUnit {
    asp,
    unit,
    serial,
    type_model,
    resource,
    size_mib,
    percent_used,
    status,
    mirrored,
    partner,
    attached,
    resume_percent,
    device,
    controller
});

wire_struct!(NonConfigured {
    device,
    resource,
    serial,
    type_model,
    size_mib,
    has_data,
    controller
});

wire_struct!(ResourceRow {
    name,
    kind,
    status,
    location,
    description
});

wire_struct!(LogEntry { time, code, text });

wire_struct!(ParityUnit {
    set,
    level,
    unit,
    serial,
    type_model,
    resource,
    controller,
    status,
    percent
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::{Privilege, SignOnError};

    #[test]
    fn values_round_trip() {
        let u = DiskUnit {
            asp: 1,
            unit: 2,
            serial: "QM00002".into(),
            type_model: "QEMU HARDDISK".into(),
            resource: "DD003".into(),
            size_mib: 2048,
            percent_used: 12,
            status: "Active".into(),
            mirrored: true,
            partner: Some(1),
            attached: true,
            resume_percent: None,
            device: "/dev/vdc".into(),
            controller: "DC01".into(),
        };
        assert_eq!(ipc::decode::<DiskUnit>(&ipc::encode(&u)).unwrap(), u);
        let p = ParityUnit {
            set: 1,
            level: "RAID 5".into(),
            unit: 3,
            status: "DPY/Rebuild".into(),
            percent: Some(40),
            ..ParityUnit::default()
        };
        assert_eq!(ipc::decode::<ParityUnit>(&ipc::encode(&p)).unwrap(), p);
        let e = SignOnError::BadPassword("empty".into());
        assert_eq!(ipc::decode::<SignOnError>(&ipc::encode(&e)).unwrap(), e);
        let p = Privilege::LOGS.with(Privilege::HARDWARE);
        assert_eq!(ipc::decode::<Privilege>(&ipc::encode(&p)).unwrap(), p);
    }
}
