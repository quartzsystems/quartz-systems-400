// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Service tools user IDs: names, privileges, Argon2 password hashes,
//! expired and disabled states, kept as one TLV in the LIC record. These
//! are not the operating system's user profiles (`MIGRATE(phase6)`);
//! they exist before the object store does.

pub mod privilege;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use licpart::Record;

pub use privilege::Privilege;

/// The TLV tag in the LIC record.
pub const TAG_USERS: u16 = 0x0102;
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
    BadPassword(&'static str),
    /// The user exists already.
    #[error("service tools user ID already exists")]
    Exists,
    /// The last user with the user ID privilege cannot lose it.
    #[error("the last user ID that can manage user IDs cannot be removed")]
    LastAdministrator,
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
        return Err(SignOnError::BadPassword("empty"));
    }
    if p.len() > PASSWORD_LEN {
        return Err(SignOnError::BadPassword("longer than 128 characters"));
    }
    if p.chars().any(char::is_control) {
        return Err(SignOnError::BadPassword("control characters"));
    }
    Ok(())
}

/// A name as stored: upper case, trimmed.
#[must_use]
pub fn normalize_name(name: &str) -> String {
    name.trim().to_ascii_uppercase()
}

impl UserDb {
    /// The shipped users: `QSECOFR` with an expired default password,
    /// `QSRV`, `11111111` and `22222222`, each with its privilege set.
    pub fn defaults(now: u64, salt: &mut dyn FnMut() -> [u8; 16]) -> Self {
        let mut db = Self::default();
        let shipped: [(&str, Privilege, bool, &str); 4] = [
            ("QSECOFR", Privilege::ALL, true, "Security officer"),
            (
                "QSRV",
                Privilege::ALL
                    .without(Privilege::SECURITY)
                    .without(Privilege::USER_IDS),
                false,
                "Service",
            ),
            (
                "11111111",
                Privilege::DISK_UNITS_OPERATIONS
                    .with(Privilege::LOGS)
                    .with(Privilege::HARDWARE)
                    .with(Privilege::IPL),
                false,
                "Basic service",
            ),
            (
                "22222222",
                Privilege::DISK_UNITS_OPERATIONS.with(Privilege::IPL),
                false,
                "Operator",
            ),
        ];
        for (name, privileges, expired, text) in shipped {
            db.users.push(ServiceUser {
                name: name.to_string(),
                privileges,
                hash: hash_password(name, salt()),
                expired,
                disabled: false,
                failures: 0,
                changed: now,
                text: text.to_string(),
            });
        }
        db
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

    fn get_mut(&mut self, name: &str) -> Option<&mut ServiceUser> {
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
            return Err(SignOnError::BadPassword("same as the old password"));
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
                "user ID must be 1 to 10 letters or digits",
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

    /// The TLV bytes.
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

    /// From TLV bytes.
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

    /// The users recorded in a LIC record, if any.
    #[must_use]
    pub fn load(record: &Record) -> Option<Self> {
        record.tlv(TAG_USERS).and_then(Self::decode)
    }

    /// Record the users in a LIC record.
    pub fn store(&self, record: &mut Record) -> Result<(), licpart::LicError> {
        record.set_tlv(TAG_USERS, self.encode())
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

    #[test]
    fn shipped_users_sign_on_and_qsecofr_must_change_first() {
        let mut s = salt;
        let mut db = UserDb::defaults(1, &mut s);
        assert_eq!(db.list().len(), 4);
        assert_eq!(db.sign_on("qsecofr", "QSECOFR"), Err(SignOnError::Expired));
        assert_eq!(db.sign_on("QSECOFR", "wrong"), Err(SignOnError::NotCorrect));
        db.change_password("QSECOFR", "QSECOFR", "Quartz1", 2, salt())
            .unwrap();
        let u = db.sign_on("QSECOFR", "Quartz1").unwrap();
        assert!(u.privileges.has(Privilege::ALL));
        let u = db.sign_on("QSRV", "QSRV").unwrap();
        assert!(!u.privileges.has(Privilege::USER_IDS));
        assert!(
            db.sign_on("22222222", "22222222")
                .unwrap()
                .privileges
                .has(Privilege::IPL)
        );
        assert_eq!(db.sign_on("NOBODY", "x"), Err(SignOnError::NotCorrect));
    }

    #[test]
    fn three_wrong_passwords_disable_the_user() {
        let mut s = salt;
        let mut db = UserDb::defaults(1, &mut s);
        assert_eq!(db.sign_on("QSRV", "a"), Err(SignOnError::NotCorrect));
        assert_eq!(db.sign_on("QSRV", "b"), Err(SignOnError::NotCorrect));
        assert_eq!(db.sign_on("QSRV", "c"), Err(SignOnError::Disabled));
        assert_eq!(db.sign_on("QSRV", "QSRV"), Err(SignOnError::Disabled));
        db.set_enabled("QSRV", true).unwrap();
        assert!(db.sign_on("QSRV", "QSRV").is_ok());
        db.reset_password("QSRV", "New1", 3, salt()).unwrap();
        assert_eq!(db.sign_on("QSRV", "New1"), Err(SignOnError::Expired));
    }

    #[test]
    fn create_delete_and_the_last_administrator() {
        let mut s = salt;
        let mut db = UserDb::defaults(1, &mut s);
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
        assert_eq!(db.delete("QSECOFR"), Err(SignOnError::LastAdministrator));
        assert_eq!(
            db.set_privileges("QSECOFR", Privilege::NONE),
            Err(SignOnError::LastAdministrator)
        );
        db.set_privileges("OPER1", Privilege::ALL).unwrap();
        db.delete("QSECOFR").unwrap();
        assert_eq!(db.list().len(), 4);
        let bytes = db.encode();
        let back = UserDb::decode(&bytes).unwrap();
        assert_eq!(back, db);
        let mut record = Record::new(0);
        db.store(&mut record).unwrap();
        assert_eq!(UserDb::load(&record).unwrap(), db);
        assert!(UserDb::decode(&bytes[..10]).is_none());
    }
}
