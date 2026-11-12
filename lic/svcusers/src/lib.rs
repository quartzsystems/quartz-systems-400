// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Service tools user IDs: names, privileges, Argon2 password hashes,
//! expired and disabled states, and the count of wrong passwords that
//! disables an ID. They are not an operating system's user profiles;
//! they exist before anything above the service tools does.
//!
//! The users are one run of bytes ([`UserDb::encode`]) the caller keeps
//! where it keeps its state, through a [`UserStore`]: a record on a
//! partition, or a file ([`FileStore`]). Which users a system ships
//! with is the caller's too ([`UserDb::push`]).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod privilege;
mod store;
#[cfg(feature = "wire")]
mod wire;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

pub use privilege::Privilege;
pub use store::{FileStore, UserStore};

/// Wrong passwords before an ID is disabled.
pub const MAX_FAILURES: u8 = 3;
/// Longest user name.
pub const NAME_LEN: usize = 10;
/// Longest password.
pub const PASSWORD_LEN: usize = 128;

/// One user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceUser {
    /// Upper-case name, at most ten characters.
    pub name: String,
    /// What the user may do.
    pub privileges: Privilege,
    /// Argon2 hash in PHC string form.
    pub hash: String,
    /// The password must be changed at the next sign-on.
    pub expired: bool,
    /// Too many wrong passwords, or disabled by an administrator.
    pub disabled: bool,
    /// Wrong passwords since the last good sign-on.
    pub failures: u8,
    /// Unix seconds the password was last set.
    pub changed: u64,
    /// Description.
    pub text: String,
}

/// Why a sign-on or a password change was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SignOnError {
    /// No such user, or the password does not match. Both read the same.
    #[error("service tools user ID or password is not correct")]
    NotCorrect,
    /// The user ID is disabled.
    #[error("service tools user ID is disabled")]
    Disabled,
    /// The password has expired and must be changed.
    #[error("password has expired; it must be changed")]
    Expired,
    /// The new password breaks a rule.
    #[error("new password is not valid: {0}")]
    BadPassword(String),
    /// The user exists already.
    #[error("service tools user ID already exists")]
    Exists,
    /// The last user with the user ID privilege cannot lose it.
    #[error("the last user ID that can manage user IDs cannot be removed")]
    LastAdministrator,
}

impl SignOnError {
    /// A number for the variant, for a message between processes.
    #[must_use]
    pub fn code(&self) -> u8 {
        match self {
            Self::NotCorrect => 0,
            Self::Disabled => 1,
            Self::Expired => 2,
            Self::BadPassword(_) => 3,
            Self::Exists => 4,
            Self::LastAdministrator => 5,
        }
    }

    /// The variant for a code and the reason of a bad password.
    #[must_use]
    pub fn from_code(code: u8, why: String) -> Self {
        match code {
            1 => Self::Disabled,
            2 => Self::Expired,
            3 => Self::BadPassword(why),
            4 => Self::Exists,
            5 => Self::LastAdministrator,
            _ => Self::NotCorrect,
        }
    }
}

/// Every user.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserDb {
    users: Vec<ServiceUser>,
}

/// Hash a password with a fresh salt.
#[must_use]
pub fn hash_password(password: &str, salt: [u8; 16]) -> String {
    let salt = SaltString::encode_b64(&salt).expect("16 bytes fit");
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("hashing cannot fail with valid parameters")
        .to_string()
}

/// Whether a password matches a hash.
#[must_use]
pub fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| {
        Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok()
    })
}

fn valid_password(p: &str) -> Result<(), SignOnError> {
    if p.is_empty() {
        return Err(SignOnError::BadPassword("empty".into()));
    }
    if p.len() > PASSWORD_LEN {
        return Err(SignOnError::BadPassword(
            "longer than 128 characters".into(),
        ));
    }
    if p.chars().any(char::is_control) {
        return Err(SignOnError::BadPassword("control characters".into()));
    }
    Ok(())
}

/// A name as stored: upper case, trimmed.
#[must_use]
pub fn normalize_name(name: &str) -> String {
    name.trim().to_ascii_uppercase()
}

impl UserDb {
    /// The same users with no password hashes: what a display needs,
    /// for a process that has no business holding the hashes.
    #[must_use]
    pub fn without_hashes(&self) -> Self {
        let mut users = self.users.clone();
        for u in &mut users {
            u.hash.clear();
        }
        Self { users }
    }

    /// Add a user as given, after the others: how a caller makes the
    /// users it ships with.
    pub fn push(&mut self, user: ServiceUser) {
        self.users.push(user);
    }

    /// Add a user as given at a place in the order.
    pub fn insert(&mut self, at: usize, user: ServiceUser) {
        self.users.insert(at.min(self.users.len()), user);
    }

    /// The users, in creation order.
    #[must_use]
    pub fn list(&self) -> &[ServiceUser] {
        &self.users
    }

    /// One user.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ServiceUser> {
        let name = normalize_name(name);
        self.users.iter().find(|u| u.name == name)
    }

    /// One user, to change.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut ServiceUser> {
        let name = normalize_name(name);
        self.users.iter_mut().find(|u| u.name == name)
    }

    /// Check a name and password. A wrong password counts toward the
    /// disable limit; a good one resets the count. An expired password
    /// signs on only far enough to be changed.
    pub fn sign_on(&mut self, name: &str, password: &str) -> Result<&ServiceUser, SignOnError> {
        let Some(u) = self.get_mut(name) else {
            return Err(SignOnError::NotCorrect);
        };
        if u.disabled {
            return Err(SignOnError::Disabled);
        }
        if !verify_password(&u.hash, password) {
            u.failures += 1;
            if u.failures >= MAX_FAILURES {
                u.disabled = true;
                return Err(SignOnError::Disabled);
            }
            return Err(SignOnError::NotCorrect);
        }
        u.failures = 0;
        if u.expired {
            return Err(SignOnError::Expired);
        }
        Ok(u)
    }

    /// Change a password, checking the old one.
    pub fn change_password(
        &mut self,
        name: &str,
        old: &str,
        new: &str,
        now: u64,
        salt: [u8; 16],
    ) -> Result<(), SignOnError> {
        valid_password(new)?;
        let Some(u) = self.get_mut(name) else {
            return Err(SignOnError::NotCorrect);
        };
        if u.disabled {
            return Err(SignOnError::Disabled);
        }
        if !verify_password(&u.hash, old) {
            return Err(SignOnError::NotCorrect);
        }
        if old == new {
            return Err(SignOnError::BadPassword("same as the old password".into()));
        }
        u.hash = hash_password(new, salt);
        u.expired = false;
        u.failures = 0;
        u.changed = now;
        Ok(())
    }

    /// Create a user with an expired password.
    pub fn create(
        &mut self,
        name: &str,
        password: &str,
        privileges: Privilege,
        text: &str,
        now: u64,
        salt: [u8; 16],
    ) -> Result<(), SignOnError> {
        let name = normalize_name(name);
        if name.is_empty()
            || name.len() > NAME_LEN
            || !name.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(SignOnError::BadPassword(
                "user ID must be 1 to 10 letters or digits".into(),
            ));
        }
        valid_password(password)?;
        if self.get(&name).is_some() {
            return Err(SignOnError::Exists);
        }
        self.users.push(ServiceUser {
            name,
            privileges,
            hash: hash_password(password, salt),
            expired: true,
            disabled: false,
            failures: 0,
            changed: now,
            text: text.trim().to_string(),
        });
        Ok(())
    }

    /// Delete a user. The last one who can manage user IDs stays.
    pub fn delete(&mut self, name: &str) -> Result<(), SignOnError> {
        let name = normalize_name(name);
        let Some(i) = self.users.iter().position(|u| u.name == name) else {
            return Err(SignOnError::NotCorrect);
        };
        if self.users[i].privileges.has(Privilege::USER_IDS) && self.administrators() == 1 {
            return Err(SignOnError::LastAdministrator);
        }
        self.users.remove(i);
        Ok(())
    }

    /// Set a new, expired password and enable the user.
    pub fn reset_password(
        &mut self,
        name: &str,
        password: &str,
        now: u64,
        salt: [u8; 16],
    ) -> Result<(), SignOnError> {
        valid_password(password)?;
        let Some(u) = self.get_mut(name) else {
            return Err(SignOnError::NotCorrect);
        };
        u.hash = hash_password(password, salt);
        u.expired = true;
        u.disabled = false;
        u.failures = 0;
        u.changed = now;
        Ok(())
    }

    /// Enable or disable a user.
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<(), SignOnError> {
        let Some(u) = self.get_mut(name) else {
            return Err(SignOnError::NotCorrect);
        };
        u.disabled = !enabled;
        if enabled {
            u.failures = 0;
        }
        Ok(())
    }

    /// Change a user's privileges.
    pub fn set_privileges(&mut self, name: &str, privileges: Privilege) -> Result<(), SignOnError> {
        let was_admin = self
            .get(name)
            .is_some_and(|u| u.privileges.has(Privilege::USER_IDS));
        if was_admin && !privileges.has(Privilege::USER_IDS) && self.administrators() == 1 {
            return Err(SignOnError::LastAdministrator);
        }
        let Some(u) = self.get_mut(name) else {
            return Err(SignOnError::NotCorrect);
        };
        u.privileges = privileges;
        Ok(())
    }

    fn administrators(&self) -> usize {
        self.users
            .iter()
            .filter(|u| u.privileges.has(Privilege::USER_IDS))
            .count()
    }

    /// The users as bytes to keep.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(
            &u16::try_from(self.users.len())
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        for u in &self.users {
            put_str(&mut out, &u.name);
            out.extend_from_slice(&u.privileges.0.to_le_bytes());
            out.push(u8::from(u.expired) | (u8::from(u.disabled) << 1));
            out.push(u.failures);
            out.extend_from_slice(&u.changed.to_le_bytes());
            put_str(&mut out, &u.hash);
            put_str(&mut out, &u.text);
        }
        out
    }

    /// From kept bytes.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader { buf: bytes, at: 0 };
        let count = r.u16()?;
        let mut users = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let name = r.str()?;
            let privileges = Privilege(r.u64()?);
            let flags = r.u8()?;
            let failures = r.u8()?;
            let changed = r.u64()?;
            let hash = r.str()?;
            let text = r.str()?;
            users.push(ServiceUser {
                name,
                privileges,
                hash,
                expired: flags & 1 != 0,
                disabled: flags & 2 != 0,
                failures,
                changed,
                text,
            });
        }
        Some(Self { users })
    }

    /// The users a store holds; `None` when it holds none it can read.
    pub fn read_from<S: UserStore + ?Sized>(store: &mut S) -> Result<Option<Self>, S::Error> {
        Ok(store.read()?.and_then(|b| Self::decode(&b)))
    }

    /// Keep the users in a store.
    pub fn write_to<S: UserStore + ?Sized>(&self, store: &mut S) -> Result<(), S::Error> {
        store.write(&self.encode())
    }
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    out.extend_from_slice(&u16::try_from(bytes.len()).unwrap_or(u16::MAX).to_le_bytes());
    out.extend_from_slice(&bytes[..bytes.len().min(usize::from(u16::MAX))]);
}

struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.buf.get(self.at..self.at + n)?;
        self.at += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn str(&mut self) -> Option<String> {
        let n = usize::from(self.u16()?);
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salt() -> [u8; 16] {
        [7; 16]
    }

    /// An officer with an expired password, a service user, an operator.
    fn shipped() -> UserDb {
        let mut db = UserDb::default();
        for (name, privileges, expired) in [
            ("OFFICER", Privilege::ALL, true),
            (
                "SERVICE",
                Privilege::ALL.without(Privilege::USER_IDS),
                false,
            ),
            ("22222222", Privilege::IPL, false),
        ] {
            db.push(ServiceUser {
                name: name.to_string(),
                privileges,
                hash: hash_password(name, salt()),
                expired,
                disabled: false,
                failures: 0,
                changed: 1,
                text: String::new(),
            });
        }
        db
    }

    #[test]
    fn users_sign_on_and_an_expired_password_must_change_first() {
        let mut db = shipped();
        assert_eq!(db.list().len(), 3);
        assert_eq!(db.sign_on("officer", "OFFICER"), Err(SignOnError::Expired));
        assert_eq!(db.sign_on("OFFICER", "wrong"), Err(SignOnError::NotCorrect));
        db.change_password("OFFICER", "OFFICER", "Quartz1", 2, salt())
            .unwrap();
        let u = db.sign_on("OFFICER", "Quartz1").unwrap();
        assert!(u.privileges.has(Privilege::ALL));
        let u = db.sign_on("SERVICE", "SERVICE").unwrap();
        assert!(!u.privileges.has(Privilege::USER_IDS));
        assert_eq!(db.sign_on("NOBODY", "x"), Err(SignOnError::NotCorrect));
        assert_eq!(
            db.change_password("OFFICER", "Quartz1", "Quartz1", 3, salt()),
            Err(SignOnError::BadPassword("same as the old password".into()))
        );
    }

    #[test]
    fn three_wrong_passwords_disable_the_user() {
        let mut db = shipped();
        assert_eq!(db.sign_on("SERVICE", "a"), Err(SignOnError::NotCorrect));
        assert_eq!(db.sign_on("SERVICE", "b"), Err(SignOnError::NotCorrect));
        assert_eq!(db.sign_on("SERVICE", "c"), Err(SignOnError::Disabled));
        assert_eq!(db.sign_on("SERVICE", "SERVICE"), Err(SignOnError::Disabled));
        db.set_enabled("SERVICE", true).unwrap();
        assert!(db.sign_on("SERVICE", "SERVICE").is_ok());
        // A good sign-on forgets the wrong ones before it.
        assert_eq!(db.sign_on("SERVICE", "a"), Err(SignOnError::NotCorrect));
        assert!(db.sign_on("SERVICE", "SERVICE").is_ok());
        assert_eq!(db.get("SERVICE").unwrap().failures, 0);
        db.reset_password("SERVICE", "New1", 3, salt()).unwrap();
        assert_eq!(db.sign_on("SERVICE", "New1"), Err(SignOnError::Expired));
    }

    #[test]
    fn create_delete_and_the_last_administrator() {
        let mut db = shipped();
        db.create(
            "OPER1",
            "pw",
            Privilege::DISK_UNITS_OPERATIONS,
            "Night shift",
            5,
            salt(),
        )
        .unwrap();
        assert_eq!(
            db.create("OPER1", "pw", Privilege::NONE, "", 5, salt()),
            Err(SignOnError::Exists)
        );
        assert!(
            db.create("toolongname1", "pw", Privilege::NONE, "", 5, salt())
                .is_err()
        );
        assert_eq!(db.delete("OFFICER"), Err(SignOnError::LastAdministrator));
        assert_eq!(
            db.set_privileges("OFFICER", Privilege::NONE),
            Err(SignOnError::LastAdministrator)
        );
        db.set_privileges("OPER1", Privilege::ALL).unwrap();
        db.delete("OFFICER").unwrap();
        assert_eq!(db.list().len(), 3);
        let bytes = db.encode();
        assert_eq!(UserDb::decode(&bytes).unwrap(), db);
        assert!(UserDb::decode(&bytes[..10]).is_none());
        // No hashes for a process that only shows the users.
        assert!(db.without_hashes().list().iter().all(|u| u.hash.is_empty()));
    }

    #[test]
    fn the_users_are_kept_in_a_state_file() {
        let dir = std::env::temp_dir().join(format!("svcusers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut file = FileStore::new(dir.join("users"));
        assert_eq!(UserDb::read_from(&mut file).unwrap(), None);
        let mut db = shipped();
        db.write_to(&mut file).unwrap();
        assert_eq!(UserDb::read_from(&mut file).unwrap().unwrap(), db);
        // A lockout survives the next start.
        for _ in 0..MAX_FAILURES {
            let _ = db.sign_on("SERVICE", "wrong");
        }
        db.write_to(&mut file).unwrap();
        let mut back = UserDb::read_from(&mut FileStore::new(dir.join("users")))
            .unwrap()
            .unwrap();
        assert_eq!(
            back.sign_on("SERVICE", "SERVICE"),
            Err(SignOnError::Disabled)
        );
        // Bytes that are not users read as none.
        std::fs::write(dir.join("users"), b"x").unwrap();
        assert_eq!(UserDb::read_from(&mut file).unwrap(), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
