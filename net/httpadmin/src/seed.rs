// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product instances: `ADMIN` in the registry with `/www/admin`,
//! made the first time the `ADMIN` job starts on a system, and
//! `NAVIGATOR` with `/www/navigator`, made by the `ADMIN` job once
//! `QS00NAV` is installed, and `ACCESSWEB` with `/www/accessweb` once
//! `QS00AFW` is. Each listens on its plain port and, with TLS,
//! on its TLS port; their jobs take the ports from `CHGHTTPA`, the
//! configuration shows them.

use std::fmt::Write as _;

use httpd::files::Files;
use httpd::instance::{self, LogFormat, Spec};
use httpd::registry::Registry;

use crate::SELF_INSTANCE;

/// The directory of the instances and the registry: the IFS makes it
/// at every IPL, owned by the administration server's profile, so the
/// `ADMIN` job can make instances under it.
pub const WWW: &str = "/www";
/// Its owner.
pub const WWW_OWNER: &str = "QTMHHTTP";
/// Its `*PUBLIC` data authority.
pub const WWW_PUBLIC: &str = "*RX";

/// The instance registry, owned by [`WWW_OWNER`] too.
pub const REGISTRY: &str = httpd::registry::PATH;

/// Navigator's instance.
pub const NAVIGATOR: &str = "NAVIGATOR";
/// The licensed program Navigator is: its instance is listed only while
/// it is installed.
pub const NAVIGATOR_PRODUCT: &str = "QS00NAV";
/// Access for Web's instance.
pub const ACCESSWEB: &str = "ACCESSWEB";
/// The licensed program Access for Web is.
pub const ACCESSWEB_PRODUCT: &str = "QS00AFW";
/// The products' instances and their licensed programs.
pub const PRODUCTS: &[(&str, &str)] = &[(NAVIGATOR, NAVIGATOR_PRODUCT), (ACCESSWEB, ACCESSWEB_PRODUCT)];

/// Make `ADMIN`'s registry entry and server root when the registry has
/// none; whether it was made. `tls_port` 0 leaves TLS out. `stamp` is
/// the time as the configuration's comment shows it.
pub fn ensure_admin(files: &dyn Files, port: u16, tls_port: u16, stamp: &str, now: u64) -> httpd::Result<bool> {
    ensure(files, SELF_INSTANCE, "HTTP Server Administration", true, (port, tls_port), stamp, now)
}

/// Make `NAVIGATOR`'s registry entry and server root when the registry
/// has none, autostart off; whether it was made.
pub fn ensure_navigator(files: &dyn Files, port: u16, tls_port: u16, stamp: &str, now: u64) -> httpd::Result<bool> {
    ensure(files, NAVIGATOR, "Navigator for QS/400", false, (port, tls_port), stamp, now)
}

/// Make `ACCESSWEB`'s registry entry and server root when the registry
/// has none, autostart off; whether it was made.
pub fn ensure_accessweb(files: &dyn Files, port: u16, tls_port: u16, stamp: &str, now: u64) -> httpd::Result<bool> {
    ensure(files, ACCESSWEB, "QS/400 Access for Web", false, (port, tls_port), stamp, now)
}

fn ensure(
    files: &dyn Files,
    name: &str,
    description: &str,
    autostart: bool,
    (port, tls_port): (u16, u16),
    stamp: &str,
    now: u64,
) -> httpd::Result<bool> {
    if Registry::load(files)?.get(name).is_some() {
        return Ok(false);
    }
    let root = instance::default_root(name);
    let spec = Spec {
        name: name.into(),
        description: description.into(),
        root: root.clone(),
        docroot: format!("{root}/htdocs"),
        sample: false,
        addr: None,
        port,
        access_log: Some((format!("{root}/logs/access_log"), LogFormat::Combined)),
        keep_days: 30,
        user: "QTMHHTTP".into(),
        autostart,
        copy_from: None,
    };
    instance::create(files, &spec, stamp, now)?;
    if tls_port != 0 {
        let path = instance::conf_path(&root);
        let mut text = String::from_utf8_lossy(&files.read(&path).map_err(|e| httpd::Error::Io(path.clone(), e.to_string()))?)
            .into_owned();
        text = text.replacen(
            &format!("Listen *:{port}\n"),
            &format!("Listen *:{port}\nListen *:{tls_port}\n"),
            1,
        );
        let _ = write!(
            text,
            "\n<VirtualHost *:{tls_port}>\n   SSLEngine On\n   SSLAppName QS400_HTTP_SERVER_{name}\n</VirtualHost>\n"
        );
        files
            .write_atomic(&path, text.as_bytes())
            .map_err(|e| httpd::Error::Io(path.clone(), e.to_string()))?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpd::files::MemFiles;

    #[test]
    fn admin_made_once() {
        let f = MemFiles::new();
        assert!(ensure_admin(&f, 2001, 2010, "11/01/26 09:00:00", 1).unwrap());
        assert!(!ensure_admin(&f, 2001, 2010, "11/01/26 09:00:00", 2).unwrap());
        let r = Registry::load(&f).unwrap();
        assert_eq!(r.get("ADMIN").unwrap().root, "/www/admin");
        let c = instance::load_conf(&f, "/www/admin").unwrap();
        let ports: Vec<(u16, bool)> = instance::ports(&c).iter().map(|p| (p.port, p.tls)).collect();
        assert_eq!(ports, [(2001, false), (2010, true)]);
    }

    #[test]
    fn navigator_made_once_without_autostart() {
        let f = MemFiles::new();
        assert!(ensure_navigator(&f, 2002, 2005, "11/01/26 09:00:00", 1).unwrap());
        assert!(!ensure_navigator(&f, 2002, 2005, "11/01/26 09:00:00", 2).unwrap());
        let r = Registry::load(&f).unwrap();
        let e = r.get("NAVIGATOR").unwrap();
        assert_eq!((e.root.as_str(), e.autostart), ("/www/navigator", false));
        let c = instance::load_conf(&f, "/www/navigator").unwrap();
        let ports: Vec<(u16, bool)> = instance::ports(&c).iter().map(|p| (p.port, p.tls)).collect();
        assert_eq!(ports, [(2002, false), (2005, true)]);
    }
}
