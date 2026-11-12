// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Service tools user IDs as QS/400 has them: the users it ships, and
//! the one TLV of the LIC record that keeps them. The IDs themselves
//! (names, privileges, Argon2 password hashes, expired and disabled
//! states, the count that disables one) are `svcusers`, which knows
//! neither. These are not the operating system's user profiles, as on
//! IBM i; they exist before the object store does.

use licpart::Record;

pub use svcusers::privilege;
pub use svcusers::{
    MAX_FAILURES, NAME_LEN, PASSWORD_LEN, Privilege, ServiceUser, SignOnError, UserDb,
    hash_password, normalize_name, verify_password,
};

/// The TLV tag in the LIC record.
pub const TAG_USERS: u16 = 0x0102;

/// What QS/400 adds to the user IDs: its shipped users and their place
/// in the LIC record.
pub trait LicUsers: Sized {
    /// The shipped users: `QSECOFR` with an expired default password,
    /// `QSRV`, `11111111` and `22222222`, each with its privilege set.
    fn defaults(now: u64, salt: &mut dyn FnMut() -> [u8; 16]) -> Self;

    /// `CHGDSTPWD PASSWORD(*DEFAULT)`: `QSECOFR` back to its shipped
    /// password, expired and enabled; made again with every privilege
    /// if it was deleted.
    fn reset_secofr(&mut self, now: u64, salt: [u8; 16]);

    /// The users recorded in a LIC record, if any.
    #[must_use]
    fn load(record: &Record) -> Option<Self>;

    /// Record the users in a LIC record.
    fn store(&self, record: &mut Record) -> Result<(), licpart::LicError>;
}

impl LicUsers for UserDb {
    fn defaults(now: u64, salt: &mut dyn FnMut() -> [u8; 16]) -> Self {
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
            db.push(ServiceUser {
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

    fn reset_secofr(&mut self, now: u64, salt: [u8; 16]) {
        const SECOFR: &str = "QSECOFR";
        if self.get(SECOFR).is_none() {
            self.insert(
                0,
                ServiceUser {
                    name: SECOFR.into(),
                    privileges: Privilege::ALL,
                    hash: String::new(),
                    expired: true,
                    disabled: false,
                    failures: 0,
                    changed: now,
                    text: "Security officer".into(),
                },
            );
        }
        let u = self.get_mut(SECOFR).expect("present");
        u.hash = hash_password(SECOFR, salt);
        u.expired = true;
        u.disabled = false;
        u.failures = 0;
        u.changed = now;
    }

    fn load(record: &Record) -> Option<Self> {
        record.tlv(TAG_USERS).and_then(Self::decode)
    }

    fn store(&self, record: &mut Record) -> Result<(), licpart::LicError> {
        record.set_tlv(TAG_USERS, self.encode())
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

    #[test]
    fn the_default_password_comes_back_and_a_deleted_qsecofr_with_it() {
        let mut s = salt;
        let mut db = UserDb::defaults(1, &mut s);
        db.change_password("QSECOFR", "QSECOFR", "Quartz1", 2, salt())
            .unwrap();
        db.reset_secofr(3, salt());
        assert_eq!(db.sign_on("QSECOFR", "QSECOFR"), Err(SignOnError::Expired));
        db.create("ADMIN2", "pw", Privilege::ALL, "", 4, salt())
            .unwrap();
        db.delete("QSECOFR").unwrap();
        db.reset_secofr(5, salt());
        assert_eq!(db.list()[0].name, "QSECOFR");
        assert!(db.list()[0].privileges.has(Privilege::ALL));
    }
}
