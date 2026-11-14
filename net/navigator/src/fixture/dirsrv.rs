// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's directory server: `dirsrv::admin::MemAdmin`'s
//! sample (`QUSRDIR` running with suffix `dc=quartz,dc=example`,
//! `DEVDIR` stopped, the two publishing agents) on `QSHQ400`, with one
//! attribute type and one object class added to the schema.

use dirsrv::admin::{DirAdmin, MemAdmin, SchemaKind};

use super::{has_special, now};

/// The sample directory server.
#[must_use]
pub fn sample() -> MemAdmin {
    let a = MemAdmin::new("QSHQ400.QUARTZ.EXAMPLE", now(), Box::new(has_special));
    for (kind, text) in [
        (
            SchemaKind::Attribute,
            "( 1.3.6.1.4.1.99999.1.1 NAME 'badgeNumber' DESC 'Building badge' SYNTAX 1.3.6.1.4.1.1466.115.121.1.15 SINGLE-VALUE )",
        ),
        (
            SchemaKind::Class,
            "( 1.3.6.1.4.1.99999.2.1 NAME 'badgeHolder' DESC 'Someone with a badge' SUP top AUXILIARY MAY badgeNumber )",
        ),
    ] {
        a.add_schema("QSECOFR", "QUSRDIR", kind, text)
            .expect("the sample definitions go in");
    }
    a
}
