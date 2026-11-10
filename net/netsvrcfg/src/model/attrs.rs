// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server attributes: one [`Settings`] in effect and one pending,
//! taken at the next start with reset or the next IPL (`QZLSCHSI`,
//! `QZLSCHSN`, `QZLSCHSG` change the pending one).

/// IBM's defaults the primary shows (`zlsl/olst-ZLSL0201-ALL.txt`).
pub mod defaults {
    /// Idle time-out in seconds.
    pub const IDLE_TIMEOUT: i32 = 6000;
    /// Opportunistic lock break time-out in seconds.
    pub const OPLOCK_TIMEOUT: i32 = 30;
    /// Browse announcement interval in milliseconds.
    pub const BROWSE_INTERVAL: i32 = 720_000;
    /// Minimum severity of administrative alerts: -1, none.
    pub const MIN_MESSAGE_SEVERITY: i32 = -1;
    /// The text QS/400 ships.
    pub const TEXT: &str = "QS/400";
}

/// The SMB2 dialects a client may negotiate, oldest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Dialect {
    /// SMB 2.0.2.
    #[default]
    Smb202,
    /// SMB 2.1.
    Smb21,
    /// SMB 3.0.
    Smb30,
    /// SMB 3.0.2.
    Smb302,
    /// SMB 3.1.1.
    Smb311,
}

impl Dialect {
    /// Every dialect, oldest first.
    pub const ALL: [Self; 5] = [
        Self::Smb202,
        Self::Smb21,
        Self::Smb30,
        Self::Smb302,
        Self::Smb311,
    ];

    /// `2.0.2`, `2.1`, ...
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Smb202 => "2.0.2",
            Self::Smb21 => "2.1",
            Self::Smb30 => "3.0",
            Self::Smb302 => "3.0.2",
            Self::Smb311 => "3.1.1",
        }
    }

    /// The wire value ([MS-SMB2] 2.2.3).
    #[must_use]
    pub fn code(self) -> u16 {
        match self {
            Self::Smb202 => 0x0202,
            Self::Smb21 => 0x0210,
            Self::Smb30 => 0x0300,
            Self::Smb302 => 0x0302,
            Self::Smb311 => 0x0311,
        }
    }

    /// The dialect named `s`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.name() == s)
    }
}

/// Every server attribute. The integer and character codes are the
/// API formats' (`ZLSL0201`, `ZLSS0100`); QS/400's own attributes
/// (`allow_netbios`, `min_dialect`, `log_signon`) are not in them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Server name, up to 15 characters, upper case.
    pub server_name: String,
    /// Domain (workgroup) name, up to 15 characters.
    pub domain: String,
    /// Text, up to 50 characters.
    pub text: String,
    /// Guest profile; empty for no guest support.
    pub guest: String,
    /// Code page for NetBIOS names; 0 is the job's ASCII equivalent.
    pub code_page: i32,
    /// Seconds before an idle session (no open files) ends; -1 never.
    pub idle_timeout: i32,
    /// Seconds a lease or oplock break waits; -1 turns them off.
    pub oplock_timeout: i32,
    /// Milliseconds between browse announcements (0 to 720000).
    pub browse_interval: i32,
    /// WINS primary server address; QS/400 has no WINS client, always
    /// empty.
    pub wins_primary: String,
    /// WINS secondary server address; always empty.
    pub wins_secondary: String,
    /// NetBIOS scope; always empty.
    pub scope: String,
    /// WINS proxy; always off.
    pub wins_proxy: bool,
    /// Whether the system name reaches the server too (`QZLSCHSN`'s
    /// allow system name).
    pub system_name_access: bool,
    /// Authentication method: 0 passwords (the only one QS/400 has),
    /// 1 Kerberos with passwords, 2 Kerberos only.
    pub auth_method: u8,
    /// Logon server role: 0 none (1, logon server, is refused).
    pub server_role: i32,
    /// Message authentication (signing): 0 and 1 offer it, 2 requires
    /// it. SMB2 always offers signing, so 0 and 1 act alike.
    pub message_auth: i32,
    /// Minimum severity of administrative alerts; -1 none (the only
    /// value QS/400 takes: SMB2 has no messenger service).
    pub min_message_severity: i32,
    /// LAN Manager authentication: 0 (1 is refused).
    pub lanman_auth: i32,
    /// Encrypted connections: 0 not required, 1 required for every
    /// connection.
    pub encryption: i32,
    /// Authorization list whose `*USE` a user needs to connect; empty
    /// for none.
    pub server_autl: String,
    /// QS/400: answer on 139 and 137/138 (NetBIOS).
    pub allow_netbios: bool,
    /// QS/400: the oldest dialect a client may negotiate.
    pub min_dialect: Dialect,
    /// QS/400: send `CPIB691` to `QSYSOPR` at each sign-on.
    pub log_signon: bool,
}

impl Settings {
    /// IBM's defaults with the server name `name` (upper case, at most
    /// 15 characters) and QS/400's text.
    #[must_use]
    pub fn new(name: &str) -> Self {
        let server: String = name.to_ascii_uppercase().chars().take(15).collect();
        Self {
            server_name: server,
            domain: String::new(),
            text: defaults::TEXT.into(),
            guest: String::new(),
            code_page: 0,
            idle_timeout: defaults::IDLE_TIMEOUT,
            oplock_timeout: defaults::OPLOCK_TIMEOUT,
            browse_interval: defaults::BROWSE_INTERVAL,
            wins_primary: String::new(),
            wins_secondary: String::new(),
            scope: String::new(),
            wins_proxy: false,
            system_name_access: false,
            auth_method: 0,
            server_role: 0,
            message_auth: 0,
            min_message_severity: defaults::MIN_MESSAGE_SEVERITY,
            lanman_auth: 0,
            encryption: 0,
            server_autl: String::new(),
            allow_netbios: false,
            min_dialect: Dialect::Smb202,
            log_signon: false,
        }
    }

    /// Whether guest logons are supported (a guest profile is set).
    #[must_use]
    pub fn guest_support(&self) -> bool {
        !self.guest.is_empty()
    }

    /// Whether leases and oplocks are granted.
    #[must_use]
    pub fn oplocks(&self) -> bool {
        self.oplock_timeout != -1
    }

    /// Whether every connection must be encrypted.
    #[must_use]
    pub fn encryption_required(&self) -> bool {
        self.encryption == 1
    }

    /// Whether signing is required.
    #[must_use]
    pub fn signing_required(&self) -> bool {
        self.message_auth == 2
    }
}

/// The attributes in effect and those pending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attributes {
    /// In effect.
    pub current: Settings,
    /// Taken at the next start with reset or the next IPL.
    pub pending: Settings,
}

impl Attributes {
    /// Both from IBM's defaults for server name `name`.
    #[must_use]
    pub fn new(name: &str) -> Self {
        let s = Settings::new(name);
        Self {
            current: s.clone(),
            pending: s,
        }
    }

    /// Make the pending values current; whether anything changed.
    pub fn apply_pending(&mut self) -> bool {
        let changed = self.current != self.pending;
        self.current = self.pending.clone();
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_primary() {
        let a = Attributes::new("qs400dev7890123456");
        assert_eq!(a.current.server_name, "QS400DEV7890123");
        assert_eq!(a.current.idle_timeout, 6000);
        assert_eq!(a.current.oplock_timeout, 30);
        assert_eq!(a.current.browse_interval, 720_000);
        assert_eq!(a.current.min_message_severity, -1);
        assert!(a.current.oplocks() && !a.current.guest_support());
        assert_eq!(a.current, a.pending);
    }

    #[test]
    fn pending_becomes_current() {
        let mut a = Attributes::new("A");
        assert!(!a.apply_pending());
        a.pending.text = "x".into();
        assert!(a.apply_pending());
        assert_eq!(a.current.text, "x");
    }

    #[test]
    fn dialects_name_and_code() {
        assert_eq!(Dialect::parse("3.1.1"), Some(Dialect::Smb311));
        assert_eq!(Dialect::Smb21.code(), 0x0210);
        assert!(Dialect::Smb30 > Dialect::Smb21);
        assert_eq!(Dialect::parse("1.0"), None);
    }
}
