// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QS*` certificate commands as library functions, over any
//! [`StoreFiles`]: the storage service runs them over the IFS, the tests
//! over [`crate::files::MemFiles`]. Stream files a command reads or
//! writes (`FROMSTMF`, `TOSTMF`) are the caller's; these take and give
//! bytes.
//!
//! - [`keystores`]: create and delete stores, the local CA.
//! - [`entries`]: create, import, export, change, remove and list
//!   certificates.
//! - [`appids`]: application IDs and their certificate assignment.
//! - [`expiry`]: the daily scan that warns before certificates expire.
//! - [`identity`]: an application ID's certificate for a server job.

pub mod appids;
pub mod entries;
pub mod expiry;
pub mod identity;
pub mod keystores;

use crate::crypto::kdf::Argon2Params;
use crate::error::{CertError, Result};
use crate::files::StoreFiles;
use crate::paths::{LOCAL_CA_STORE, SYSTEM_NAME, SYSTEM_STORE};
use crate::stash::Sealer;
use crate::store::CertStore;

/// The store name commands use for the local CA's store.
pub const LOCAL_CA_NAME: &str = "*LCLCA";

/// What every command works with.
pub struct Ctx<'a> {
    /// The store files, read and written as the system.
    pub files: &'a dyn StoreFiles,
    /// The LIC key that seals store passwords; `None` when the system
    /// has none (stashes cannot be read or written).
    pub sealer: Option<&'a dyn Sealer>,
    /// Unix seconds.
    pub now: u64,
    /// Key derivation costs for new stores.
    pub params: Argon2Params,
}

/// A store as a command names it: `*SYSTEM`, `*LCLCA` or a path, with
/// its password or `None` for the stash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keystore {
    /// The store file's path.
    pub path: String,
    /// The password typed; `None` opens through the stash.
    pub password: Option<String>,
}

impl Keystore {
    /// From a command's `KEYSTORE` and `PASSWORD` values (`*STASH` or
    /// blank for the stash).
    pub fn new(name: &str, password: &str) -> Self {
        let n = name.trim();
        let path = if n.eq_ignore_ascii_case(SYSTEM_NAME) {
            SYSTEM_STORE.to_string()
        } else if n.eq_ignore_ascii_case(LOCAL_CA_NAME) {
            LOCAL_CA_STORE.to_string()
        } else {
            n.to_string()
        };
        let p = password.trim();
        let password = (!p.is_empty() && !p.eq_ignore_ascii_case("*STASH")).then(|| p.to_string());
        Self { path, password }
    }

    /// The `*SYSTEM` store through its stash.
    pub fn system() -> Self {
        Self::new(SYSTEM_NAME, "")
    }

    /// Whether this is the `*SYSTEM` store, whose labels application IDs
    /// name.
    pub fn is_system(&self) -> bool {
        self.path == SYSTEM_STORE
    }

    /// The name shown in messages: `*SYSTEM`, `*LCLCA` or the path.
    pub fn shown(&self) -> &str {
        shown(&self.path)
    }
}

impl Ctx<'_> {
    /// The sealer, or why there is none.
    pub fn sealer(&self) -> Result<&dyn Sealer> {
        self.sealer.ok_or_else(|| {
            CertError::Invalid(
                "this system has no key for store passwords; type the store's password".into(),
            )
        })
    }

    /// Open a store with its password or its stash.
    pub fn open(&self, ks: &Keystore) -> Result<CertStore> {
        if let Some(p) = &ks.password {
            return CertStore::open(self.files, &ks.path, p);
        }
        if !self.files.exists(&ks.path)? {
            return Err(CertError::NotFound(ks.path.clone()));
        }
        CertStore::open_stashed(self.files, &ks.path, self.sealer()?)
    }
}

/// The message identifier a refusal is sent as (`QTCPMSG`).
pub fn msgid(e: &CertError) -> &'static str {
    match e {
        CertError::NotFound(_) => msgs::NOT_FOUND,
        CertError::Exists(_) => msgs::EXISTS,
        CertError::BadPassword => msgs::BAD_PASSWORD,
        CertError::LabelNotFound(_) => msgs::NO_LABEL,
        CertError::LabelExists(_) => msgs::LABEL_EXISTS,
        CertError::NotRegistered(_) => msgs::NOT_REGISTERED,
        CertError::AlreadyRegistered(_) => msgs::REGISTERED,
        CertError::Unseal => msgs::NO_STASH,
        _ => msgs::FAILED,
    }
}

/// A refusal as the escape message a command sends: its identifier and
/// data. `store` is the store the command named, as shown.
pub fn refusal(e: &CertError, store: &str) -> Note {
    let id = msgid(e);
    let data = match e {
        CertError::NotFound(p) | CertError::Exists(p) => vec![shown(p).to_string()],
        CertError::BadPassword | CertError::Unseal => vec![store.to_string()],
        CertError::LabelNotFound(l) | CertError::LabelExists(l) => vec![l.clone()],
        CertError::NotRegistered(a) | CertError::AlreadyRegistered(a) => vec![a.clone()],
        other => vec![other.to_string()],
    };
    Note { msgid: id, data }
}

/// A store path as messages show it.
fn shown(path: &str) -> &str {
    match path {
        SYSTEM_STORE => SYSTEM_NAME,
        LOCAL_CA_STORE => LOCAL_CA_NAME,
        p => p,
    }
}

/// The message identifiers of the certificate commands, in `QTCPMSG`.
pub mod msgs {
    /// Store created: path.
    pub const STORE_CREATED: &str = "TCP1C00";
    /// Store deleted: path.
    pub const STORE_DELETED: &str = "TCP1C01";
    /// Certificate created: label, store.
    pub const CREATED: &str = "TCP1C02";
    /// Request created: label, store.
    pub const REQUEST_CREATED: &str = "TCP1C03";
    /// Imported: label, store.
    pub const IMPORTED: &str = "TCP1C04";
    /// Exported: label, stream file.
    pub const EXPORTED: &str = "TCP1C05";
    /// Removed: label, store.
    pub const REMOVED: &str = "TCP1C06";
    /// Changed: label.
    pub const CHANGED: &str = "TCP1C07";
    /// Assigned: label, application.
    pub const ASSIGNED: &str = "TCP1C08";
    /// Assignment removed: application.
    pub const UNASSIGNED: &str = "TCP1C09";
    /// Application added.
    pub const APP_ADDED: &str = "TCP1C0A";
    /// Application removed.
    pub const APP_REMOVED: &str = "TCP1C0B";
    /// Application changed.
    pub const APP_CHANGED: &str = "TCP1C0C";
    /// Expiry check done: warnings sent.
    pub const CHECKED: &str = "TCP1C0D";
    /// No such store.
    pub const NOT_FOUND: &str = "TCP1C10";
    /// The store exists.
    pub const EXISTS: &str = "TCP1C11";
    /// Wrong password.
    pub const BAD_PASSWORD: &str = "TCP1C12";
    /// No such label.
    pub const NO_LABEL: &str = "TCP1C13";
    /// Label in use.
    pub const LABEL_EXISTS: &str = "TCP1C14";
    /// Application not registered.
    pub const NOT_REGISTERED: &str = "TCP1C15";
    /// Application registered already.
    pub const REGISTERED: &str = "TCP1C16";
    /// Anything else: the reason.
    pub const FAILED: &str = "TCP1C17";
    /// The stash does not open.
    pub const NO_STASH: &str = "TCP1C18";
    /// Not authorized.
    pub const NOT_AUTHORIZED: &str = "TCP1C19";
    /// Expiry warning: label, store, days.
    pub const EXPIRES: &str = "TCP1C20";
    /// Expired: label, store.
    pub const EXPIRED: &str = "TCP1C21";
}

/// One message a command sends: identifier and the values of its
/// substitution variables, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// `TCP1Cnn`.
    pub msgid: &'static str,
    /// The variables' values.
    pub data: Vec<String>,
}

impl Note {
    /// A note.
    pub fn new(msgid: &'static str, data: &[&str]) -> Self {
        Self {
            msgid,
            data: data.iter().map(|s| (*s).to_string()).collect(),
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::files::MemFiles;
    use crate::stash::KeySealer;

    pub const NOW: u64 = 1_800_000_000;
    pub const DAY: u64 = 86_400;

    pub fn fast() -> Argon2Params {
        Argon2Params {
            m_cost: 64,
            t_cost: 1,
            p_cost: 1,
        }
    }

    pub struct World {
        pub files: MemFiles,
        pub sealer: KeySealer,
    }

    impl World {
        pub fn new() -> Self {
            Self {
                files: MemFiles::new(),
                sealer: KeySealer::for_tests(),
            }
        }

        pub fn ctx(&self) -> Ctx<'_> {
            self.at(NOW)
        }

        pub fn at(&self, now: u64) -> Ctx<'_> {
            Ctx {
                files: &self.files,
                sealer: Some(&self.sealer),
                now,
                params: fast(),
            }
        }

        /// A world with a stashed `*SYSTEM` store and no roots.
        pub fn with_system() -> Self {
            let w = Self::new();
            keystores::create(&w.ctx(), &Keystore::new("*SYSTEM", "system1"), true, false).unwrap();
            w
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    #[test]
    fn keystore_names() {
        let s = Keystore::new("*system", "*STASH");
        assert!(s.is_system());
        assert_eq!(s.password, None);
        assert_eq!(s.shown(), "*SYSTEM");
        let c = Keystore::new("*LCLCA", "secret1");
        assert_eq!(c.path, LOCAL_CA_STORE);
        assert_eq!(c.password.as_deref(), Some("secret1"));
        assert_eq!(Keystore::new("/home/x/my.kdb", " ").shown(), "/home/x/my.kdb");
    }

    #[test]
    fn opening_needs_the_store_and_the_key() {
        let w = World::new();
        let ks = Keystore::system();
        assert!(matches!(w.ctx().open(&ks), Err(CertError::NotFound(_))));
        let w = World::with_system();
        assert!(w.ctx().open(&ks).is_ok());
        let no_key = Ctx {
            sealer: None,
            ..w.ctx()
        };
        assert!(matches!(no_key.open(&ks), Err(CertError::Invalid(_))));
        assert!(no_key.open(&Keystore::new("*SYSTEM", "system1")).is_ok());
        let e = w.ctx().open(&Keystore::new("*SYSTEM", "wrong1")).err().unwrap();
        assert_eq!(refusal(&e, "*SYSTEM"), Note::new(msgs::BAD_PASSWORD, &["*SYSTEM"]));
        let e = World::new().ctx().open(&ks).err().unwrap();
        assert_eq!(refusal(&e, "*SYSTEM"), Note::new(msgs::NOT_FOUND, &["*SYSTEM"]));
    }
}
