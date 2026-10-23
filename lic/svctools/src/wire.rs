// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The process-message encoding of what SST asks the storage service
//! for: the disk facts, the logs, the hardware table, privileges and
//! refusals (feature `wire`).

use ipc::{Dec, Wire, WireError, wire_struct};

use crate::env::{DiskUnit, LogEntry, NonConfigured, ResourceRow};
use crate::users::{Privilege, SignOnError};

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
    device
});

wire_struct!(NonConfigured {
    device,
    resource,
    serial,
    type_model,
    size_mib,
    has_data
});

wire_struct!(ResourceRow {
    name,
    kind,
    status,
    location,
    description
});

wire_struct!(LogEntry { time, code, text });

impl Wire for Privilege {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(Self(u64::get(d)?))
    }
}

impl Wire for SignOnError {
    fn put(&self, out: &mut Vec<u8>) {
        self.code().put(out);
        match self {
            Self::BadPassword(why) => why.put(out),
            _ => String::new().put(out),
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let code = u8::get(d)?;
        let why = String::get(d)?;
        Ok(Self::from_code(code, why))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        assert_eq!(ipc::decode::<DiskUnit>(&ipc::encode(&u)).unwrap(), u);
        let e = SignOnError::BadPassword("empty".into());
        assert_eq!(ipc::decode::<SignOnError>(&ipc::encode(&e)).unwrap(), e);
        let p = Privilege::LOGS.with(Privilege::HARDWARE);
        assert_eq!(ipc::decode::<Privilege>(&ipc::encode(&p)).unwrap(), p);
    }
}
