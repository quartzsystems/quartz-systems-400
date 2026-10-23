// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with DST environment`: active service tools, system devices,
//! service tools user IDs, and the LIC-side system values.

use console::Screen;

use crate::dst::NOT_AUTHORIZED;
use crate::dst::common::{Notice, Options, Submenu, guarded, parse_options};
use crate::env::{DstEnv, UserChange};
use crate::panel::{Flow, Panel};
use crate::ui::{self, FormLine};
use crate::users::{Privilege, ServiceUser};

/// The `Work with DST Environment` menu.
#[must_use]
pub fn work_with_dst_environment(privileges: Privilege) -> Submenu {
    let can_users = privileges.has(Privilege::USER_IDS);
    let can_sysvals = privileges.has(Privilege::SYSTEM_VALUES);
    Submenu::new(
        "dst-env",
        "Work with DST Environment",
        vec![
            (
                1,
                "Active service tools",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(Notice::new(
                        "dst-env-active",
                        "Active Service Tools",
                        vec![
                            "Service tool                        Started by".into(),
                            format!("Dedicated service tools             {}", env.console_type()),
                        ],
                    )))
                })),
            ),
            (
                2,
                "System devices",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(Notice::new(
                        "dst-env-devices",
                        "System Devices",
                        vec![
                            format!("Console type . . . . . :  {}", env.console_type()),
                            String::new(),
                            "Option 6 of the DST menu chooses the local or the serial console;".into(),
                            "an operations console over the LAN comes with a later phase.".into(),
                        ],
                    )))
                })),
            ),
            (
                3,
                "Service tools user IDs",
                Some(Box::new(move |env| {
                    if can_users {
                        Flow::Push(Box::new(user_list(env)))
                    } else {
                        Flow::Error(NOT_AUTHORIZED.into())
                    }
                })),
            ),
            (
                4,
                "System values",
                Some(Box::new(move |env| {
                    if can_sysvals {
                        Flow::Push(Box::new(sysval_list(env)))
                    } else {
                        Flow::Error(NOT_AUTHORIZED.into())
                    }
                })),
            ),
            (5, "Service tools device IDs", None),
        ],
    )
}

/// `User ID     Status    Description`.
pub const USER_HEADINGS: &str = "User ID     Status     Description";

fn user_row(u: &ServiceUser) -> String {
    let status = if u.disabled {
        "Disabled"
    } else if u.expired {
        "Expired"
    } else {
        "Enabled"
    };
    format!("{:<11} {:<10} {}", u.name, status, u.text)
}

/// The user list with its options, rebuilt from the users every time
/// it is shown so changes made under it appear on return.
pub struct UserList;

/// The user list.
#[must_use]
pub fn user_list(_env: &mut dyn DstEnv) -> UserList {
    UserList
}

impl Panel for UserList {
    fn name(&self) -> &'static str {
        "dst-users"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let rows: Vec<String> = env.users().list().iter().map(user_row).collect();
        ui::list(
            "Work with Service Tools User IDs",
            &env.system_name(),
            "Type options, press Enter.  2=Change privileges  3=Delete  4=Display  5=Enable  6=Disable  7=Reset password",
            &format!("Opt  {USER_HEADINGS}"),
            &rows,
            "F3=Exit   F6=Create   F12=Cancel",
        )
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let chosen = match parse_options(fields, &[2, 3, 4, 5, 6, 7]) {
            Ok(c) => c,
            Err(e) => return Flow::Error(e),
        };
        let (row, opt) = chosen[0];
        let Some(name) = env.users().list().get(row).map(|u| u.name.clone()) else {
            return Flow::Error("Entry not found".into());
        };
        match opt {
            2 => Flow::Push(Box::new(Privileges::new(name))),
            3 => {
                let lines = vec![
                    format!("Service tools user ID {name} will be deleted."),
                    String::new(),
                    "Press F10 to delete it, F12 to keep it.".into(),
                ];
                let change = UserChange::Delete { name: name.clone() };
                let done = format!("User ID {name} deleted");
                guarded(
                    env,
                    "dst-user-delete-confirm",
                    "Confirm Delete of Service Tools User ID",
                    lines,
                    0,
                    Box::new(move |env: &mut dyn DstEnv| env.change_user(&change).map(|()| done.clone())),
                )
            }
            4 => Flow::Push(Box::new(display_user(env, &name))),
            5 | 6 => {
                let enable = opt == 5;
                let word = if enable { "enabled" } else { "disabled" };
                let lines = vec![
                    format!("Service tools user ID {name} will be {word}."),
                    String::new(),
                    "Press F10 to go on, F12 to leave it as it is.".into(),
                ];
                let change = UserChange::Enable {
                    name: name.clone(),
                    enable,
                };
                let done = format!("User ID {name} {word}");
                guarded(
                    env,
                    "dst-user-enable-confirm",
                    "Confirm Change of Service Tools User ID",
                    lines,
                    0,
                    Box::new(move |env: &mut dyn DstEnv| env.change_user(&change).map(|()| done.clone())),
                )
            }
            _ => Flow::Push(Box::new(ResetPassword::new(name))),
        }
    }

    fn function(&mut self, key: u8, _env: &mut dyn DstEnv) -> Flow {
        match key {
            6 => Flow::Push(Box::new(CreateUser)),
            3 => Flow::PopAll,
            12 => Flow::Pop,
            _ => Flow::Stay,
        }
    }
}

fn display_user(env: &mut dyn DstEnv, name: &str) -> Notice {
    let lines = match env.users().get(name) {
        Some(u) => {
            let mut l = vec![
                format!("User ID  . . . . . . . :  {}", u.name),
                format!("Description  . . . . . :  {}", u.text),
                format!(
                    "Status . . . . . . . . :  {}",
                    if u.disabled { "Disabled" } else { "Enabled" }
                ),
                format!(
                    "Password expired . . . :  {}",
                    if u.expired { "Yes" } else { "No" }
                ),
                format!("Wrong passwords  . . . :  {}", u.failures),
                format!("Password changed . . . :  {}", u.changed),
                String::new(),
                "Privileges:".into(),
            ];
            for (p, text) in Privilege::NAMES {
                l.push(format!(
                    "  {}  {text}",
                    if u.privileges.has(p) { "X" } else { " " }
                ));
            }
            l
        }
        None => vec!["User ID not found".into()],
    };
    Notice::new("dst-user-display", "Display Service Tools User ID", lines)
}

/// Create a user: name, password, description; privileges follow.
#[derive(Default)]
pub struct CreateUser;

impl Panel for CreateUser {
    fn name(&self) -> &'static str {
        "dst-user-create"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        ui::form(
            "Create Service Tools User ID",
            &env.system_name(),
            &[
                FormLine::new("User ID", 10, ""),
                FormLine::hidden("Password", 32),
                FormLine::hidden("Password (to verify)", 32),
                FormLine::new("Description", 50, ""),
            ],
            "F3=Exit   F12=Cancel",
        )
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let name = fields.first().map_or("", String::as_str).trim().to_string();
        let pw = fields.get(1).map_or("", String::as_str);
        let again = fields.get(2).map_or("", String::as_str);
        let text = fields.get(3).map_or("", String::as_str);
        if pw != again {
            return Flow::Error("Passwords do not match".into());
        }
        // A new ID has no privileges: nothing to confirm until they come.
        let change = UserChange::Create {
            name: name.clone(),
            password: pw.to_string(),
            text: text.to_string(),
        };
        match env.change_user(&change) {
            Ok(()) => Flow::Replace(Box::new(Privileges::new(name.to_ascii_uppercase()))),
            Err(e) => Flow::Error(e),
        }
    }
}

/// Change a user's privileges: X beside each one.
pub struct Privileges {
    name: String,
}

impl Privileges {
    /// For `name`.
    #[must_use]
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl Panel for Privileges {
    fn name(&self) -> &'static str {
        "dst-user-privileges"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let have = env
            .users()
            .get(&self.name)
            .map(|u| u.privileges)
            .unwrap_or_default();
        let mut s = Screen::new();
        ui::header(
            &mut s,
            "Change Service Tools User Privileges",
            &env.system_name(),
        );
        s.put(
            ui::BODY_ROW,
            ui::BODY_COL,
            &format!("User ID . . . . . :  {}", self.name),
            console::Attr::GREEN,
        );
        s.put(
            ui::BODY_ROW + 2,
            ui::BODY_COL,
            "Type X beside each privilege to grant, blank to revoke, press Enter.",
            console::Attr::GREEN,
        );
        for (i, (p, text)) in Privilege::NAMES.iter().enumerate() {
            let row = ui::BODY_ROW + 4 + i;
            s.field(row, ui::BODY_COL + 1, 1, false);
            if have.has(*p) {
                s.put(row, ui::BODY_COL + 1, "X", console::Attr::INPUT);
            }
            s.put(row, ui::BODY_COL + 4, text, console::Attr::GREEN);
        }
        ui::keys(&mut s, "F3=Exit   F12=Cancel");
        s.cursor = (ui::BODY_ROW + 3, ui::BODY_COL);
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let mut p = Privilege::NONE;
        for (i, (bit, _)) in Privilege::NAMES.iter().enumerate() {
            if fields
                .get(i)
                .is_some_and(|f| f.trim().eq_ignore_ascii_case("x"))
            {
                p = p.with(*bit);
            }
        }
        let name = self.name.clone();
        let mut lines = vec![
            format!("The privileges of service tools user ID {name} will be:"),
            String::new(),
        ];
        for (bit, text) in Privilege::NAMES {
            if p.has(bit) {
                lines.push(format!("  {text}"));
            }
        }
        if p == Privilege::NONE {
            lines.push("  (None)".into());
        }
        let change = UserChange::Privileges {
            name: name.clone(),
            privileges: p,
        };
        let done = format!("Privileges of {name} changed");
        guarded(
            env,
            "dst-user-privileges-confirm",
            "Confirm Change of Service Tools User Privileges",
            lines,
            1,
            Box::new(move |env: &mut dyn DstEnv| env.change_user(&change).map(|()| done.clone())),
        )
    }
}

/// Set a new (expired) password for a user.
pub struct ResetPassword {
    name: String,
}

impl ResetPassword {
    /// For `name`.
    #[must_use]
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl Panel for ResetPassword {
    fn name(&self) -> &'static str {
        "dst-user-reset"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        ui::form(
            "Reset Service Tools User Password",
            &env.system_name(),
            &[
                FormLine::new("User ID", 10, self.name.clone()),
                FormLine::hidden("New password", 32),
                FormLine::hidden("New password (to verify)", 32),
            ],
            "F3=Exit   F12=Cancel",
        )
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let pw = fields.get(1).map_or("", String::as_str);
        let again = fields.get(2).map_or("", String::as_str);
        if pw != again {
            return Flow::Error("Passwords do not match".into());
        }
        let name = self.name.clone();
        let lines = vec![
            format!("The password of service tools user ID {name} will be replaced."),
            "The new one has expired: it is changed at the next sign-on.".into(),
        ];
        let change = UserChange::Reset {
            name: name.clone(),
            password: pw.to_string(),
        };
        let done = format!("Password of {name} reset; it must be changed at the next sign-on");
        guarded(
            env,
            "dst-user-reset-confirm",
            "Confirm Reset of Service Tools User Password",
            lines,
            1,
            Box::new(move |env: &mut dyn DstEnv| env.change_user(&change).map(|()| done.clone())),
        )
    }
}

/// The LIC-side system values, with 2=Change.
pub fn sysval_list(env: &mut dyn DstEnv) -> Options {
    let values = env.lic_sysvals();
    let names: Vec<String> = values.iter().map(|(n, _)| n.clone()).collect();
    let rows: Vec<String> = values.iter().map(|(n, v)| format!("{n:<11} {v}")).collect();
    Options::new(
        "dst-sysvals",
        "Work with System Values",
        "Type options, press Enter.  2=Change",
        "Opt  Name        Value",
        rows,
        &[2],
        "F3=Exit   F12=Cancel",
        Box::new(move |chosen: &[(usize, u8)], env: &mut dyn DstEnv| {
            let name = names[chosen[0].0].clone();
            let value = env
                .lic_sysvals()
                .into_iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| v)
                .unwrap_or_default();
            Flow::Push(Box::new(ChangeSysval { name, value }))
        }),
        None,
    )
}

/// Change one system value.
pub struct ChangeSysval {
    name: String,
    value: String,
}

impl Panel for ChangeSysval {
    fn name(&self) -> &'static str {
        "dst-sysval-change"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        ui::form(
            "Change System Value",
            &env.system_name(),
            &[
                FormLine::new("System value", 10, self.name.clone()),
                FormLine::new("New value", 50, self.value.clone()),
            ],
            "F3=Exit   F12=Cancel",
        )
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let value = fields.get(1).map_or("", String::as_str).trim();
        match env.set_lic_sysval(&self.name, value) {
            Ok(()) => Flow::Unwind(2, Some((format!("{} changed", self.name), false))),
            Err(e) => Flow::Error(e),
        }
    }
}
