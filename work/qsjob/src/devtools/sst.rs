// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRSST` in the job: the `sst` crate's menus over DST's panels, shown
//! on the job's display, with an environment ([`SstEnv`]) that asks the
//! storage service (requests 420-432) for the disks, the logs, the
//! hardware table and the service tools user IDs. The service checks
//! `*SERVICE` and the signed-on user's privileges on every request; the
//! job checks `*SERVICE` first so the refusal is a message.

use std::collections::BTreeMap;

use cmd::Resolved;
use mi::Job;
use stg::Client;
use svctools::env::{DiskUnit, LogEntry, NonConfigured, ResourceRow, SecurityFlags, UserChange};
use svctools::users::SignOnError;
use svctools::{DstEnv, Privilege, UserDb};
use tn5250::ds::aid;
use tn5250::screen::Read;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::apply;
use crate::panels::shows_panels;

/// Run `STRSST`.
pub fn run(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let _ = (job, out);
    if !shows_panels(host, r) {
        return Err(Fail::new(
            "CPF9898",
            "STRSST needs a display; it cannot run in this job.",
        ));
    }
    let me = host.link.borrow_mut().with(Client::whoami);
    me.require(sec::profile::SpecialAuth::SERVICE, "use the system service tools")
        .map_err(|e| Fail::new("CPF2222", e.to_string()))?;
    let mut surface = Surface::new(host.clone());
    let mut env = SstEnv::new(host);
    let mut runner = sst::start();
    loop {
        let grid = runner.screen(&mut env);
        let Some((key, fields)) = show(&mut surface, &grid) else {
            break;
        };
        let outcome = match key {
            Key::Enter => runner.enter(&fields, &mut env),
            Key::F(n) => runner.function(n, &mut env),
            Key::PageDown => runner.function(8, &mut env),
            Key::PageUp => runner.function(7, &mut env),
            Key::Other => None,
        };
        if outcome.is_some() {
            break;
        }
    }
    host.link.borrow_mut().with(Client::sst_sign_off);
    Ok(String::new())
}

/// The key that ended a read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Enter,
    F(u8),
    PageDown,
    PageUp,
    Other,
}

/// Show a DST screen; the key and the input fields in tab order, or
/// `None` when the display is gone.
fn show(surface: &mut Surface, grid: &sst::Grid) -> Option<(Key, Vec<String>)> {
    let screen = sst::to_uim(grid);
    surface.with_model(|m| apply(&screen, m));
    surface.flush(Read::MdtFields);
    let (a, _) = surface.wait()?;
    let key = match a {
        aid::ENTER | aid::AUTO_ENTER => Key::Enter,
        aid::ROLL_UP => Key::PageDown,
        aid::ROLL_DOWN => Key::PageUp,
        other => aid::function_of(other).map_or(Key::Other, Key::F),
    };
    let mut by_name = BTreeMap::new();
    surface.with_model(|m| {
        for f in &screen.fields {
            let v = m
                .field_index_at(f.row, f.col)
                .map(|i| m.fields()[i].text())
                .unwrap_or_default();
            if let uim::render::Target::Var(n) = &f.target {
                by_name.insert(n.clone(), v);
            }
        }
    });
    let fields = (0..screen.fields.len())
        .map(|i| by_name.remove(&sst::field_name(i)).unwrap_or_default())
        .collect();
    Some((key, fields))
}

/// The service tools' view of the system from a job: everything through
/// the storage service.
pub struct SstEnv {
    host: Host,
    system: String,
    started_by: String,
    users: UserDb,
}

impl SstEnv {
    fn new(host: &Host) -> Self {
        let s = &host.start;
        Self {
            host: host.clone(),
            system: crate::display::dds::system_name(host),
            started_by: format!("{:06}/{}/{}", s.number, s.user, s.name),
            users: UserDb::default(),
        }
    }

    fn disk_change(&mut self, op: &str, unit: u16, device: &str) -> Result<u16, String> {
        let (op, device) = (op.to_string(), device.to_string());
        self.host
            .link
            .borrow_mut()
            .with(|l| l.sst_disk_change(op, unit, device))
    }

    fn disks(&mut self) -> (Vec<DiskUnit>, Vec<NonConfigured>) {
        self.host
            .link
            .borrow_mut()
            .with(Client::sst_disks)
            .unwrap_or_default()
    }
}

impl DstEnv for SstEnv {
    fn system_name(&self) -> String {
        self.system.clone()
    }

    fn now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }

    fn random(&mut self) -> [u8; 16] {
        // Passwords are hashed by the service; nothing here salts.
        [0; 16]
    }

    fn users(&mut self) -> &mut UserDb {
        let bytes = self
            .host
            .link
            .borrow_mut()
            .with(Client::sst_users)
            .unwrap_or_default();
        self.users = UserDb::decode(&bytes).unwrap_or_default();
        &mut self.users
    }

    fn save_users(&mut self) -> Result<(), String> {
        // Every change already went to the service.
        Ok(())
    }

    fn confirm_changes(&self) -> bool {
        true
    }

    fn sign_on(&mut self, user: &str, password: &str) -> Result<Privilege, SignOnError> {
        let (u, p) = (user.to_string(), password.to_string());
        self.host
            .link
            .borrow_mut()
            .with(|l| l.sst_sign_on(u, p))
    }

    fn change_password(&mut self, user: &str, old: &str, new: &str) -> Result<Privilege, String> {
        let (u, o, n) = (user.to_string(), old.to_string(), new.to_string());
        self.host
            .link
            .borrow_mut()
            .with(|l| l.sst_change_password(u, o, n))
    }

    fn change_user(&mut self, change: &UserChange) -> Result<(), String> {
        let (op, name, password, text, privileges) = match change {
            UserChange::Create {
                name,
                password,
                text,
            } => ("create", name, password.clone(), text.clone(), Privilege::NONE),
            UserChange::Delete { name } => {
                ("delete", name, String::new(), String::new(), Privilege::NONE)
            }
            UserChange::Enable { name, enable } => (
                if *enable { "enable" } else { "disable" },
                name,
                String::new(),
                String::new(),
                Privilege::NONE,
            ),
            UserChange::Privileges { name, privileges } => (
                "privileges",
                name,
                String::new(),
                String::new(),
                *privileges,
            ),
            UserChange::Reset { name, password } => {
                ("reset", name, password.clone(), String::new(), Privilege::NONE)
            }
        };
        let (op, name) = (op.to_string(), name.clone());
        self.host
            .link
            .borrow_mut()
            .with(|l| l.sst_user_change(op, name, password, text, privileges))
    }

    fn disk_units(&mut self) -> Vec<DiskUnit> {
        self.disks().0
    }

    fn nonconfigured(&mut self) -> Vec<NonConfigured> {
        self.disks().1
    }

    fn add_unit(&mut self, device: &str) -> Result<u16, String> {
        self.disk_change("add", 0, device)
    }

    fn remove_unit(&mut self, unit: u16) -> Result<(), String> {
        self.disk_change("remove", unit, "").map(|_| ())
    }

    fn start_mirror(&mut self, unit: u16, device: &str) -> Result<u16, String> {
        self.disk_change("mirror", unit, device)
    }

    fn stop_mirror(&mut self, keep: u16) -> Result<(), String> {
        self.disk_change("unmirror", keep, "").map(|_| ())
    }

    fn suspend(&mut self, unit: u16) -> Result<(), String> {
        self.disk_change("suspend", unit, "").map(|_| ())
    }

    fn resume(&mut self, unit: u16) -> Result<(), String> {
        self.disk_change("resume", unit, "").map(|_| ())
    }

    fn replace(&mut self, unit: u16, device: &str) -> Result<(), String> {
        self.disk_change("replace", unit, device).map(|_| ())
    }

    fn parity_capable(&mut self) -> Vec<String> {
        self.host
            .link
            .borrow_mut()
            .with(Client::sst_disk_facts)
            .map(|(_, p)| p)
            .unwrap_or_default()
    }

    fn asp_threshold(&mut self) -> u8 {
        self.host
            .link
            .borrow_mut()
            .with(Client::sst_disk_facts)
            .map_or(90, |(t, _)| t)
    }

    fn set_asp_threshold(&mut self, percent: u8) -> Result<(), String> {
        self.disk_change("threshold", u16::from(percent), "")
            .map(|_| ())
    }

    fn console_type(&self) -> String {
        self.started_by.clone()
    }

    fn lic_info(&mut self) -> Vec<(String, String)> {
        let cpus = self
            .host
            .link
            .borrow_mut()
            .with(Client::sst_resources)
            .map_or(1, |(_, n)| n);
        vec![(sst::capacity::PROCESSORS.to_string(), cpus.to_string())]
    }

    fn activity_log(&mut self) -> Vec<LogEntry> {
        self.host
            .link
            .borrow_mut()
            .with(Client::sst_logs)
            .map(|(pal, _)| pal)
            .unwrap_or_default()
    }

    fn lic_log(&mut self) -> Vec<String> {
        self.host
            .link
            .borrow_mut()
            .with(Client::sst_logs)
            .map_or_else(|e| vec![e], |(_, log)| log)
    }

    fn read_page(&mut self, unit: u16, lba: u64) -> Result<Vec<u8>, String> {
        self.host
            .link
            .borrow_mut()
            .with(|l| l.sst_storage(unit, lba, String::new()))
    }

    fn object_header(&mut self, id: &str) -> Result<Vec<u8>, String> {
        let id = id.to_string();
        self.host
            .link
            .borrow_mut()
            .with(|l| l.sst_storage(0, 0, id))
    }

    fn resources(&mut self) -> Vec<ResourceRow> {
        self.host
            .link
            .borrow_mut()
            .with(Client::sst_resources)
            .map(|(rows, _)| rows)
            .unwrap_or_default()
    }

    fn security_flags(&mut self) -> SecurityFlags {
        self.host
            .link
            .borrow_mut()
            .with(Client::sst_security)
            .map(svctools::dst::security::decode)
            .unwrap_or_default()
    }

    fn set_security_flags(&mut self, flags: SecurityFlags) -> Result<(), String> {
        let b = svctools::dst::security::encode(flags);
        self.host
            .link
            .borrow_mut()
            .with(|l| l.sst_set_security(b))
    }
}
