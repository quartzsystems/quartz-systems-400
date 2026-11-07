// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Policies: every function the product offers, and whether a user may
//! use it. A function is checked at the user profile, then its groups,
//! then `*PUBLIC`; a Deny at any level denies, an Allow allows when
//! nothing denies, and a function nobody set is allowed. Changing
//! policies is the system's: it needs `*SECADM`. Policies come on top
//! of object authority, which the commands behind the pages check.

use crate::env::Env;
use crate::env::policy::Setting;

/// The `*PUBLIC` level.
pub const PUBLIC: &str = "*PUBLIC";

/// Sign on to Access for Web.
pub const SIGNON: &str = "signon";
/// Start 5250 sessions.
pub const S5250_START: &str = "5250.start";
/// Save session settings.
pub const S5250_SAVE: &str = "5250.save";
/// Use auto sign-on.
pub const S5250_AUTOSIGNON: &str = "5250.autosignon";
/// Print screen.
pub const S5250_PRINT: &str = "5250.print";
/// Browse tables.
pub const DB_BROWSE: &str = "db.browse";
/// Insert, update and delete records.
pub const DB_WRITE: &str = "db.write";
/// Run SQL - SELECT.
pub const DB_SELECT: &str = "db.select";
/// Run SQL - any statement.
pub const DB_SQL: &str = "db.sql";
/// Copy data to table.
pub const DB_COPY_TO: &str = "db.copyto";
/// Copy data from table.
pub const DB_COPY_FROM: &str = "db.copyfrom";
/// Save requests.
pub const DB_SAVE: &str = "db.save";
/// Share requests.
pub const DB_SHARE: &str = "db.share";
/// Browse files.
pub const FILES_BROWSE: &str = "files.browse";
/// Upload files.
pub const FILES_UPLOAD: &str = "files.upload";
/// Download files.
pub const FILES_DOWNLOAD: &str = "files.download";
/// Edit text files.
pub const FILES_EDIT: &str = "files.edit";
/// Delete files.
pub const FILES_DELETE: &str = "files.delete";
/// Browse outside home directory.
pub const FILES_OUTSIDE: &str = "files.outside";
/// View printer output as text.
pub const PRINT_TEXT: &str = "print.text";
/// View printer output as PDF.
pub const PRINT_PDF: &str = "print.pdf";
/// Hold, release, delete own output.
pub const PRINT_OWN: &str = "print.own";
/// Move output between queues.
pub const PRINT_MOVE: &str = "print.move";
/// Work with other users' output.
pub const PRINT_OTHERS: &str = "print.others";
/// Display own messages.
pub const MSGS_DISPLAY: &str = "msgs.display";
/// Send messages.
pub const MSGS_SEND: &str = "msgs.send";
/// Send inquiry messages.
pub const MSGS_INQUIRY: &str = "msgs.inquiry";
/// Reply to inquiry messages.
pub const MSGS_REPLY: &str = "msgs.reply";
/// Display own jobs.
pub const JOBS_DISPLAY: &str = "jobs.display";
/// End own jobs.
pub const JOBS_END: &str = "jobs.end";
/// Display server jobs.
pub const JOBS_SERVER: &str = "jobs.server";
/// End server jobs.
pub const JOBS_END_SERVER: &str = "jobs.endserver";
/// Run commands.
pub const CMD_RUN: &str = "cmd.run";
/// Prompt commands.
pub const CMD_PROMPT: &str = "cmd.prompt";
/// Run saved (shared) commands.
pub const CMD_SAVED: &str = "cmd.saved";
/// Save commands.
pub const CMD_SAVE: &str = "cmd.save";
/// Change own preferences.
pub const PREFS_CHANGE: &str = "prefs.change";
/// Change policies (administrator): the system's, by `*SECADM`.
pub const POLICIES_CHANGE: &str = "policies.change";

/// A group of functions on the Policies page.
#[derive(Clone, Copy, Debug)]
pub struct Section {
    /// The form's name part (`p_database_1`).
    pub key: &'static str,
    /// `Database`.
    pub label: &'static str,
    /// Function key and label.
    pub functions: &'static [(&'static str, &'static str)],
}

/// Every function, as the Policies page lists them.
pub const SECTIONS: &[Section] = &[
    Section {
        key: "signon",
        label: "Sign on",
        functions: &[(SIGNON, "Sign on to Access for Web")],
    },
    Section {
        key: "5250",
        label: "5250",
        functions: &[
            (S5250_START, "Start 5250 sessions"),
            (S5250_SAVE, "Save session settings"),
            (S5250_AUTOSIGNON, "Use auto sign-on"),
            (S5250_PRINT, "Print screen"),
        ],
    },
    Section {
        key: "database",
        label: "Database",
        functions: &[
            (DB_BROWSE, "Browse tables"),
            (DB_WRITE, "Insert, update and delete records"),
            (DB_SELECT, "Run SQL - SELECT"),
            (DB_SQL, "Run SQL - any statement"),
            (DB_COPY_TO, "Copy data to table"),
            (DB_COPY_FROM, "Copy data from table"),
            (DB_SAVE, "Save requests"),
            (DB_SHARE, "Share requests"),
        ],
    },
    Section {
        key: "files",
        label: "Files",
        functions: &[
            (FILES_BROWSE, "Browse files"),
            (FILES_UPLOAD, "Upload files"),
            (FILES_DOWNLOAD, "Download files"),
            (FILES_EDIT, "Edit text files"),
            (FILES_DELETE, "Delete files"),
            (FILES_OUTSIDE, "Browse outside home directory"),
        ],
    },
    Section {
        key: "print",
        label: "Print",
        functions: &[
            (PRINT_TEXT, "View printer output as text"),
            (PRINT_PDF, "View printer output as PDF"),
            (PRINT_OWN, "Hold, release, delete own output"),
            (PRINT_MOVE, "Move output between queues"),
            (PRINT_OTHERS, "Work with other users' output"),
        ],
    },
    Section {
        key: "messages",
        label: "Messages",
        functions: &[
            (MSGS_DISPLAY, "Display own messages"),
            (MSGS_SEND, "Send messages"),
            (MSGS_INQUIRY, "Send inquiry messages"),
            (MSGS_REPLY, "Reply to inquiry messages"),
        ],
    },
    Section {
        key: "jobs",
        label: "Jobs",
        functions: &[
            (JOBS_DISPLAY, "Display own jobs"),
            (JOBS_END, "End own jobs"),
            (JOBS_SERVER, "Display server jobs"),
            (JOBS_END_SERVER, "End server jobs"),
        ],
    },
    Section {
        key: "commands",
        label: "Commands",
        functions: &[
            (CMD_RUN, "Run commands"),
            (CMD_PROMPT, "Prompt commands"),
            (CMD_SAVED, "Run saved (shared) commands"),
            (CMD_SAVE, "Save commands"),
        ],
    },
    Section {
        key: "customize",
        label: "Customize",
        functions: &[
            (PREFS_CHANGE, "Change own preferences"),
            (POLICIES_CHANGE, "Change policies (administrator)"),
        ],
    },
];

/// A function's section and label, `None` for an unknown key.
pub fn function(key: &str) -> Option<(&'static Section, &'static str)> {
    SECTIONS
        .iter()
        .find_map(|s| s.functions.iter().find(|f| f.0 == key).map(|f| (s, f.1)))
}

/// `Jobs - End server jobs`: a function as messages name it.
pub fn title(key: &str) -> String {
    function(key).map_or_else(|| key.to_string(), |(s, l)| format!("{} - {l}", s.label))
}

/// Who decided a function's setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetBy {
    /// Nobody set it: allowed.
    Default,
    /// The system decides (changing policies needs `*SECADM`).
    System,
    /// A user profile's own setting.
    User(String),
    /// A group profile's setting.
    Group(String),
    /// `*PUBLIC`'s setting.
    Public,
}

impl SetBy {
    /// As the Set by column shows it.
    pub fn shown(&self) -> String {
        match self {
            SetBy::Default => "Default".into(),
            SetBy::System => "System".into(),
            SetBy::User(u) => format!("User {u}"),
            SetBy::Group(g) => format!("Group {g}"),
            SetBy::Public => PUBLIC.into(),
        }
    }
}

/// A function's effective setting and who set it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effective {
    /// Allow or Deny.
    pub setting: Setting,
    /// Who decided.
    pub by: SetBy,
}

impl Effective {
    /// Whether the function may be used.
    pub fn allowed(&self) -> bool {
        self.setting == Setting::Allow
    }
}

/// One level of the search: who, and its own settings.
pub struct Level {
    /// Who the settings are.
    pub by: SetBy,
    /// Function key and setting.
    pub settings: Vec<(String, Setting)>,
}

/// The levels checked for `subject`: the profile, its groups,
/// `*PUBLIC`. `*PUBLIC` alone has only itself.
pub fn levels(env: &Env, subject: &str) -> Vec<Level> {
    let mut out = Vec::new();
    if subject != PUBLIC {
        let is_group = env.system.profiles().iter().any(|p| p.name == subject && p.group);
        out.push(Level {
            by: if is_group { SetBy::Group(subject.into()) } else { SetBy::User(subject.into()) },
            settings: env.policy.settings(subject),
        });
        if !is_group {
            for g in env.system.groups(subject) {
                out.push(Level {
                    settings: env.policy.settings(&g),
                    by: SetBy::Group(g),
                });
            }
        }
    }
    out.push(Level {
        by: SetBy::Public,
        settings: env.policy.settings(PUBLIC),
    });
    out
}

/// The effective setting of `function` over `levels`; `secadm` says
/// whether the subject has `*SECADM` (for [`POLICIES_CHANGE`]).
pub fn resolve(levels: &[Level], function: &str, secadm: bool) -> Effective {
    if function == POLICIES_CHANGE {
        return Effective {
            setting: if secadm { Setting::Allow } else { Setting::Deny },
            by: SetBy::System,
        };
    }
    let at = |want: Setting| {
        levels
            .iter()
            .find(|l| l.settings.iter().any(|(f, s)| f == function && *s == want))
            .map(|l| l.by.clone())
    };
    if let Some(by) = at(Setting::Deny) {
        return Effective {
            setting: Setting::Deny,
            by,
        };
    }
    if let Some(by) = at(Setting::Allow) {
        return Effective {
            setting: Setting::Allow,
            by,
        };
    }
    Effective {
        setting: Setting::Allow,
        by: SetBy::Default,
    }
}

/// Whether `user` has `*SECADM` (`*ALLOBJ` does not give it).
pub fn secadm(env: &Env, user: &str) -> bool {
    env.system.special(user).iter().any(|s| s == "*SECADM")
}

/// `policy(user, function)`: what every page asks before it shows or
/// runs an action.
pub fn policy(env: &Env, user: &str, function: &str) -> Effective {
    resolve(&levels(env, user), function, secadm(env, user))
}

/// Whether `user` may use `function`.
pub fn allowed(env: &Env, user: &str, function: &str) -> bool {
    policy(env, user, function).allowed()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(by: SetBy, s: &[(&str, Setting)]) -> Level {
        Level {
            by,
            settings: s.iter().map(|(f, s)| ((*f).to_string(), *s)).collect(),
        }
    }

    #[test]
    fn deny_wins_at_any_level() {
        let lv = [
            level(SetBy::User("U".into()), &[(DB_WRITE, Setting::Allow)]),
            level(SetBy::Group("G".into()), &[(DB_WRITE, Setting::Deny), (DB_SQL, Setting::Allow)]),
            level(SetBy::Public, &[(DB_SQL, Setting::Deny), (FILES_EDIT, Setting::Allow)]),
        ];
        assert_eq!(resolve(&lv, DB_WRITE, false).by, SetBy::Group("G".into()));
        assert_eq!(resolve(&lv, DB_SQL, false), Effective { setting: Setting::Deny, by: SetBy::Public });
        assert_eq!(resolve(&lv, FILES_EDIT, false), Effective { setting: Setting::Allow, by: SetBy::Public });
        assert_eq!(resolve(&lv, DB_BROWSE, false), Effective { setting: Setting::Allow, by: SetBy::Default });
        assert!(!resolve(&lv, POLICIES_CHANGE, false).allowed());
        assert_eq!(resolve(&lv, POLICIES_CHANGE, true).by, SetBy::System);
    }

    #[test]
    fn every_function_once() {
        let all: Vec<&str> = SECTIONS.iter().flat_map(|s| s.functions.iter().map(|f| f.0)).collect();
        for (i, f) in all.iter().enumerate() {
            assert!(!all[i + 1..].contains(f), "{f} twice");
        }
        assert_eq!(title(JOBS_END_SERVER), "Jobs - End server jobs");
    }
}
