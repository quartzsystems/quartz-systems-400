// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DST sign-on and the forced password change.

use console::Screen;

use crate::dst::menu::MainMenu;
use crate::env::DstEnv;
use crate::panel::{Flow, Panel};
use crate::ui::{self, FormLine};
use crate::users::{SignOnError, normalize_name};

/// Sign-on: user and password.
#[derive(Default)]
pub struct SignOn {
    user: String,
}

impl Panel for SignOn {
    fn name(&self) -> &'static str {
        "dst-signon"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        ui::form(
            "Dedicated Service Tools (DST) Sign On",
            &env.system_name(),
            &[
                FormLine::new("Service tools user", 10, self.user.clone()),
                FormLine::hidden("Service tools password", 32),
            ],
            "F3=Exit   F12=Cancel",
        )
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let user = normalize_name(fields.first().map_or("", String::as_str));
        let password = fields.get(1).map_or("", String::as_str);
        self.user.clone_from(&user);
        let result = env.users().sign_on(&user, password).map(|u| u.privileges);
        // Failure counts and disables are durable.
        let _ = env.save_users();
        match result {
            Ok(privileges) => Flow::Reset(Box::new(MainMenu::new(user, privileges))),
            Err(SignOnError::Expired) => Flow::Push(Box::new(ChangePassword::new(user))),
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
}

impl ChangePassword {
    /// For `user`.
    #[must_use]
    pub fn new(user: String) -> Self {
        Self { user }
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
        ui::message(
            &mut s,
            "Password has expired and must be changed before DST can be used",
            false,
        );
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let old = fields.get(1).map_or("", String::as_str);
        let new = fields.get(2).map_or("", String::as_str);
        let again = fields.get(3).map_or("", String::as_str);
        if new != again {
            return Flow::Error("New passwords do not match".into());
        }
        let now = env.now();
        let salt = env.random();
        let result = env.users().change_password(&self.user, old, new, now, salt);
        match result {
            Ok(()) => {
                if let Err(e) = env.save_users() {
                    return Flow::Error(format!("Password changed but not saved: {e}"));
                }
                let privileges = env
                    .users()
                    .get(&self.user)
                    .map(|u| u.privileges)
                    .unwrap_or_default();
                Flow::Reset(Box::new(MainMenu::new(self.user.clone(), privileges)))
            }
            Err(e) => Flow::Error(message(&e)),
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
