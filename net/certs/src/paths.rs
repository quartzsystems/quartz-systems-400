// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where the stores live: IBM i's default paths, holding our format.
//!
//! The files sit at the paths IBM's DCM uses so programs that name a
//! keyring file (`GSK_KEYRING_FILE`, `SSL_Init`) find it. The content is
//! the Quartz format (docs/CERTS.md); a real KDB is refused as corrupt.

/// The `*SYSTEM` store.
pub const SYSTEM_STORE: &str = "/QIBM/UserData/ICSS/Cert/Server/DEFAULT.KDB";
/// The local CA's store.
pub const LOCAL_CA_STORE: &str = "/QIBM/UserData/ICSS/Cert/CertAuth/DEFAULT.KDB";
/// The local CA's certificate as PEM, for clients to install.
pub const LOCAL_CA_CERT: &str = "/QIBM/UserData/ICSS/Cert/CertAuth/DEFAULT.TXT";
/// The `*OBJECTSIGNING` store (named for completeness; unused yet).
pub const OBJECT_SIGNING_STORE: &str = "/QIBM/UserData/ICSS/Cert/Signing/SIGNING.KDB";
/// The application definitions (a Quartz file; IBM keeps these in the
/// registration facility).
pub const APP_DEFINITIONS: &str = "/QIBM/UserData/ICSS/Cert/Server/QSAPPDEF.JSON";
/// The special store name programs and commands use for the system store.
pub const SYSTEM_NAME: &str = "*SYSTEM";

/// The stash file of a store: its password sealed by the system's key,
/// beside it with the extension `.RDB` (IBM's stash extension).
pub fn stash_path(store: &str) -> String {
    let base = match store.rfind('.') {
        Some(dot) if !store[dot..].contains('/') => &store[..dot],
        _ => store,
    };
    format!("{base}.RDB")
}

/// A store name as programs give it: `*SYSTEM` or a path.
pub fn resolve(name: &str) -> &str {
    if name.trim().eq_ignore_ascii_case(SYSTEM_NAME) {
        SYSTEM_STORE
    } else {
        name.trim()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stash_sits_beside_the_store() {
        assert_eq!(
            stash_path(SYSTEM_STORE),
            "/QIBM/UserData/ICSS/Cert/Server/DEFAULT.RDB"
        );
        assert_eq!(stash_path("/home/a.b/store"), "/home/a.b/store.RDB");
        assert_eq!(resolve("*system"), SYSTEM_STORE);
        assert_eq!(resolve("/x/y.kdb"), "/x/y.kdb");
    }
}
