// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QAZLSCFG`: the attributes, each as a current record (its tag) and
//! a pending one (the tag with bit 15 set). The tags are
//! `docs/NETSERVER.md` 4.4's table.

use super::FileError;
use super::tlv::{self, Record, flag, int, put, put_flag, put_int, text};
use crate::model::attrs::{Attributes, Dialect, Settings};

/// The file's magic.
pub const MAGIC: &[u8; 8] = b"QSZLSCFG";

/// Set on a pending value's tag.
pub const PENDING: u16 = 0x8000;

/// The attribute tags.
pub mod tag {
    /// Server name.
    pub const SERVER_NAME: u16 = 0x0001;
    /// Domain.
    pub const DOMAIN: u16 = 0x0002;
    /// Text.
    pub const TEXT: u16 = 0x0003;
    /// Guest profile.
    pub const GUEST: u16 = 0x0004;
    /// Code page.
    pub const CODE_PAGE: u16 = 0x0005;
    /// Idle time-out.
    pub const IDLE_TIMEOUT: u16 = 0x0006;
    /// Oplock time-out.
    pub const OPLOCK_TIMEOUT: u16 = 0x0007;
    /// Browse interval.
    pub const BROWSE_INTERVAL: u16 = 0x0008;
    /// WINS primary.
    pub const WINS_PRIMARY: u16 = 0x0009;
    /// WINS secondary.
    pub const WINS_SECONDARY: u16 = 0x000A;
    /// Scope.
    pub const SCOPE: u16 = 0x000B;
    /// WINS proxy.
    pub const WINS_PROXY: u16 = 0x000C;
    /// Allow the system name.
    pub const SYSTEM_NAME_ACCESS: u16 = 0x000D;
    /// Authentication method.
    pub const AUTH_METHOD: u16 = 0x000E;
    /// Logon server role.
    pub const SERVER_ROLE: u16 = 0x000F;
    /// Message authentication.
    pub const MESSAGE_AUTH: u16 = 0x0010;
    /// Minimum message severity.
    pub const MIN_MESSAGE_SEVERITY: u16 = 0x0011;
    /// LAN Manager authentication.
    pub const LANMAN_AUTH: u16 = 0x0012;
    /// Encrypted connections.
    pub const ENCRYPTION: u16 = 0x0013;
    /// Server authorization list.
    pub const SERVER_AUTL: u16 = 0x0014;
    /// QS/400: allow NetBIOS.
    pub const ALLOW_NETBIOS: u16 = 0x0015;
    /// QS/400: oldest SMB2 dialect.
    pub const MIN_DIALECT: u16 = 0x0016;
    /// QS/400: log sign-ons.
    pub const LOG_SIGNON: u16 = 0x0017;
}

fn records(s: &Settings, bit: u16, out: &mut Vec<Record>) {
    out.extend([
        put(tag::SERVER_NAME | bit, &s.server_name),
        put(tag::DOMAIN | bit, &s.domain),
        put(tag::TEXT | bit, &s.text),
        put(tag::GUEST | bit, &s.guest),
        put_int(tag::CODE_PAGE | bit, s.code_page),
        put_int(tag::IDLE_TIMEOUT | bit, s.idle_timeout),
        put_int(tag::OPLOCK_TIMEOUT | bit, s.oplock_timeout),
        put_int(tag::BROWSE_INTERVAL | bit, s.browse_interval),
        put(tag::WINS_PRIMARY | bit, &s.wins_primary),
        put(tag::WINS_SECONDARY | bit, &s.wins_secondary),
        put(tag::SCOPE | bit, &s.scope),
        put_flag(tag::WINS_PROXY | bit, s.wins_proxy),
        put_flag(tag::SYSTEM_NAME_ACCESS | bit, s.system_name_access),
        put_int(tag::AUTH_METHOD | bit, i32::from(s.auth_method)),
        put_int(tag::SERVER_ROLE | bit, s.server_role),
        put_int(tag::MESSAGE_AUTH | bit, s.message_auth),
        put_int(tag::MIN_MESSAGE_SEVERITY | bit, s.min_message_severity),
        put_int(tag::LANMAN_AUTH | bit, s.lanman_auth),
        put_int(tag::ENCRYPTION | bit, s.encryption),
        put(tag::SERVER_AUTL | bit, &s.server_autl),
        put_flag(tag::ALLOW_NETBIOS | bit, s.allow_netbios),
        put(tag::MIN_DIALECT | bit, s.min_dialect.name()),
        put_flag(tag::LOG_SIGNON | bit, s.log_signon),
    ]);
}

/// Set the attribute `t` (pending bit cleared) of `s`; false when the
/// tag is not one this release knows.
fn apply(s: &mut Settings, t: u16, full: u16, v: &[u8]) -> Result<bool, FileError> {
    match t {
        tag::SERVER_NAME => s.server_name = text(full, v)?,
        tag::DOMAIN => s.domain = text(full, v)?,
        tag::TEXT => s.text = text(full, v)?,
        tag::GUEST => s.guest = text(full, v)?,
        tag::CODE_PAGE => s.code_page = int(full, v)?,
        tag::IDLE_TIMEOUT => s.idle_timeout = int(full, v)?,
        tag::OPLOCK_TIMEOUT => s.oplock_timeout = int(full, v)?,
        tag::BROWSE_INTERVAL => s.browse_interval = int(full, v)?,
        tag::WINS_PRIMARY => s.wins_primary = text(full, v)?,
        tag::WINS_SECONDARY => s.wins_secondary = text(full, v)?,
        tag::SCOPE => s.scope = text(full, v)?,
        tag::WINS_PROXY => s.wins_proxy = flag(full, v)?,
        tag::SYSTEM_NAME_ACCESS => s.system_name_access = flag(full, v)?,
        tag::AUTH_METHOD => {
            s.auth_method = u8::try_from(int(full, v)?).map_err(|_| FileError::Value(full))?;
        }
        tag::SERVER_ROLE => s.server_role = int(full, v)?,
        tag::MESSAGE_AUTH => s.message_auth = int(full, v)?,
        tag::MIN_MESSAGE_SEVERITY => s.min_message_severity = int(full, v)?,
        tag::LANMAN_AUTH => s.lanman_auth = int(full, v)?,
        tag::ENCRYPTION => s.encryption = int(full, v)?,
        tag::SERVER_AUTL => s.server_autl = text(full, v)?,
        tag::ALLOW_NETBIOS => s.allow_netbios = flag(full, v)?,
        tag::MIN_DIALECT => {
            s.min_dialect = Dialect::parse(&text(full, v)?).ok_or(FileError::Value(full))?;
        }
        tag::LOG_SIGNON => s.log_signon = flag(full, v)?,
        _ => return Ok(false),
    }
    Ok(true)
}

/// The file's bytes.
pub fn write(c: &crate::model::config::Config) -> Result<Vec<u8>, FileError> {
    let mut r = Vec::new();
    records(&c.attrs.current, 0, &mut r);
    records(&c.attrs.pending, PENDING, &mut r);
    r.extend(c.unknown.iter().cloned());
    tlv::write(MAGIC, &r)
}

/// The attributes and the records not known. Attributes the file does
/// not have keep IBM's defaults for server `system_name`.
pub fn read(b: &[u8], system_name: &str) -> Result<(Attributes, Vec<Record>), FileError> {
    let mut a = Attributes::new(system_name);
    let mut unknown = Vec::new();
    for (t, v) in tlv::read(MAGIC, b)? {
        let target = if t & PENDING == 0 {
            &mut a.current
        } else {
            &mut a.pending
        };
        if !apply(target, t & !PENDING, t, &v)? {
            unknown.push((t, v));
        }
    }
    Ok((a, unknown))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::Config;

    #[test]
    fn current_and_pending_differ_and_round_trip() {
        let mut c = Config::shipped("QS400");
        c.attrs.pending.server_name = "NEWNAME".into();
        c.attrs.pending.min_dialect = Dialect::Smb311;
        c.attrs.pending.guest = "QGUEST".into();
        c.unknown.push((0x0400, b"later".to_vec()));
        let b = write(&c).expect("writes");
        let (a, u) = read(&b, "X").expect("reads");
        assert_eq!(a, c.attrs);
        assert_eq!(u, c.unknown);
    }

    #[test]
    fn missing_records_take_defaults() {
        let b = tlv::write(MAGIC, &[put(tag::TEXT, "hello")]).expect("writes");
        let (a, _) = read(&b, "sys1").expect("reads");
        assert_eq!(a.current.text, "hello");
        assert_eq!(a.current.server_name, "SYS1");
        assert_eq!(a.pending.text, "QS/400");
    }

    #[test]
    fn a_bad_value_is_an_error() {
        let b = tlv::write(MAGIC, &[put(tag::IDLE_TIMEOUT | PENDING, "soon")]).expect("writes");
        assert_eq!(read(&b, "X"), Err(FileError::Value(0x8006)));
    }
}
