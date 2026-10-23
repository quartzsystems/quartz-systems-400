// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service tools sign-on and the forced password change. DST
//! shows them with its own title and goes on to its main menu; SST
//! (`STRSST`) gives another title and its own menu.

use console::Screen;

use crate::dst::menu::MainMenu;
use crate::env::DstEnv;
use crate::panel::{Flow, Panel};
use crate::ui::{self, FormLine};
use crate::users::{Privilege, SignOnError, normalize_name};

/// The menu a signed-on user lands on.
pub type Home = fn(String, Privilege) -> Box<dyn Panel>;

/// DST's main menu.
#[must_use]
pub fn dst_home(user: String, privileges: Privilege) -> Box<dyn Panel> {
    Box::new(MainMenu::new(user, privileges))
}

/// What differs between the tools that sign on.
#[derive(Clone, Copy)]
pub struct Tool {
    /// The sign-on title.
    pub title: &'static str,
    /// The note on the password change display.
    pub expired: &'static str,
    /// Where a sign-on goes.
    pub home: Home,
}

/// DST.
pub const DST: Tool = Tool {
    title: "Dedicated Service Tools (DST) Sign On",
    expired: "Password has expired and must be changed before DST can be used",
    home: dst_home,
};

/// Sign-on: user and password.
pub struct SignOn {
    user: String,
    tool: Tool,
}

impl Default for SignOn {
    fn default() -> Self {
        Self::new(DST)
    }
}

impl SignOn {
    /// For a tool.
    #[must_use]
    pub fn new(tool: Tool) -> Self {
        Self {
            user: String::new(),
            tool,
        }
    }
}

impl Panel for SignOn {
    fn name(&self) -> &'static str {
        "dst-signon"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        // Both leaders run to one column and both fields are 10 wide.
        ui::form_wide(
            self.tool.title,
            &env.system_name(),
            &[
                FormLine::new("Service tools user", 10, self.user.clone()),
                FormLine::hidden("Service tools password", 10),
            ],
            "F3=Exit   F12=Cancel",
            40,
        )
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let user = normalize_name(fields.first().map_or("", String::as_str));
        let password = fields.get(1).map_or("", String::as_str);
        self.user.clone_from(&user);
        match env.sign_on(&user, password) {
            Ok(privileges) => Flow::Reset((self.tool.home)(user, privileges)),
            Err(SignOnError::Expired) => {
                Flow::Push(Box::new(ChangePassword::for_tool(user, self.tool)))
            }
            Err(e) => Flow::Error(message(&e)),
        }
    }
}

/// The text for a refused sign-on or password change.
#[must_use]
pub fn message(e: &SignOnError) -> String {
    match e {
        SignOnError::NotCorrect => "User ID or password is not correct".into(),
        SignOnError::Disabled => {
            "User ID is disabled; another user with the user ID privilege can enable it".into()
        }
        SignOnError::Expired => "Password has expired and must be changed".into(),
        SignOnError::BadPassword(why) => format!("New password is not valid: {why}"),
        SignOnError::Exists => "User ID already exists".into(),
        SignOnError::LastAdministrator => {
            "The last user ID that can manage user IDs must keep that privilege".into()
        }
    }
}

/// Change an expired password: old, new, and new again.
pub struct ChangePassword {
    user: String,
    tool: Tool,
}

impl ChangePassword {
    /// For `user`, going on to DST.
    #[must_use]
    pub fn new(user: String) -> Self {
        Self::for_tool(user, DST)
    }

    /// For `user`, going on to `tool`'s menu.
    #[must_use]
    pub fn for_tool(user: String, tool: Tool) -> Self {
        Self { user, tool }
    }
}

impl Panel for ChangePassword {
    fn name(&self) -> &'static str {
        "dst-change-password"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = ui::form(
            "Change Service Tools User Password",
            &env.system_name(),
            &[
                FormLine::new("Service tools user", 10, self.user.clone()),
                FormLine::hidden("Current password", 32),
                FormLine::hidden("New password", 32),
                FormLine::hidden("New password (to verify)", 32),
            ],
            "F3=Exit   F12=Cancel",
        );
        ui::message(&mut s, self.tool.expired, false);
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let old = fields.get(1).map_or("", String::as_str);
        let new = fields.get(2).map_or("", String::as_str);
        let again = fields.get(3).map_or("", String::as_str);
        if new != again {
            return Flow::Error("New passwords do not match".into());
        }
        match env.change_password(&self.user, old, new) {
            Ok(privileges) => Flow::Reset((self.tool.home)(self.user.clone(), privileges)),
            Err(e) => Flow::Error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::env::MemEnv;
    use crate::panel::Outcome;

    fn f(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn qsecofr_changes_its_password_then_reaches_the_menu() {
        let mut env = MemEnv::new("QS400");
        let mut r = crate::dst::start();
        assert_eq!(r.top(), "dst-signon");
        assert_eq!(r.enter(&f(&["qsecofr", "nope"]), &mut env), None);
        assert!(
            r.screen(&mut env)
                .dump()
                .contains("User ID or password is not correct")
        );
        assert_eq!(r.enter(&f(&["qsecofr", "QSECOFR"]), &mut env), None);
        assert_eq!(r.top(), "dst-change-password");
        assert_eq!(
            r.enter(&f(&["QSECOFR", "QSECOFR", "a", "b"]), &mut env),
            None
        );
        assert!(r.screen(&mut env).dump().contains("do not match"));
        assert_eq!(
            r.enter(&f(&["QSECOFR", "QSECOFR", "Quartz1", "Quartz1"]), &mut env),
            None
        );
        assert_eq!(r.top(), "dst-main");
        assert!(env.saves >= 2);
        assert_eq!(r.function(3, &mut env), Some(Outcome::Exit));
    }

    #[test]
    fn three_failures_disable_and_are_saved() {
        let mut env = MemEnv::new("QS400");
        let mut r = crate::dst::start();
        for _ in 0..3 {
            r.enter(&f(&["QSRV", "x"]), &mut env);
        }
        assert!(r.screen(&mut env).dump().contains("User ID is disabled"));
        assert!(env.users.get("QSRV").unwrap().disabled);
        assert_eq!(env.saves, 3);
    }
}
