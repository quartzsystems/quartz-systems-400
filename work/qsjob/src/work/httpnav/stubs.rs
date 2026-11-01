// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The areas not live yet: their pages show empty lists. A change still
//! runs its command as the user.

use navigator::env::config::{Config, Disks, Resource, SysVal, Time};
use navigator::env::db::Db;
use navigator::env::net::{HostEntry, Interface, Net, Route, Server, TcpConfig};
use navigator::env::perf::{DiskActivity, Perf, SysActivity};
use navigator::env::sec::Security;
use navigator::env::sw::Software;
use qsweb::msg::Message;

use super::Link;

/// System values, disks, hardware, time, TCP/IP and performance:
/// nothing read yet (TODO: live over `sysval_panel_list`, `DSKSTS`,
/// the hardware resource service, `tcp`, `SYSACT`).
pub struct Pending(pub Link);

impl Config for Pending {
    fn sysvals(&self, _user: &str) -> Vec<SysVal> {
        Vec::new()
    }

    fn disks(&self, _user: &str, _reset: bool) -> Disks {
        Disks::default()
    }

    fn resources(&self, _user: &str) -> Vec<Resource> {
        Vec::new()
    }

    fn time(&self, _user: &str) -> Time {
        Time::default()
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.0.run(user, command)
    }
}

impl Net for Pending {
    fn config(&self, _user: &str) -> TcpConfig {
        TcpConfig::default()
    }

    fn interfaces(&self, _user: &str) -> Vec<Interface> {
        Vec::new()
    }

    fn routes(&self, _user: &str) -> Vec<Route> {
        Vec::new()
    }

    fn hosts(&self, _user: &str) -> Vec<HostEntry> {
        Vec::new()
    }

    fn servers(&self, _user: &str) -> Vec<Server> {
        Vec::new()
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.0.run(user, command)
    }
}

impl Perf for Pending {
    fn system(&self, _user: &str) -> SysActivity {
        SysActivity::default()
    }

    fn disks(&self, _user: &str) -> DiskActivity {
        DiskActivity::default()
    }
}

/// The database, the audit journal and software: no methods yet.
pub struct Missing;

impl Db for Missing {}

impl Security for Missing {}

impl Software for Missing {}
