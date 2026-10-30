// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where IBM i keeps OpenSSH's files (recorded on V7R5M0: the product
//! under `ProdData`, its `etc` a link to the `UserData` directory), and
//! the default configuration written at first start.

/// The configuration directory.
pub const ETC_DIR: &str = "/QOpenSys/QIBM/UserData/SC1/OpenSSH/etc";
/// `sshd_config`.
pub const CONFIG_FILE: &str = "/QOpenSys/QIBM/UserData/SC1/OpenSSH/etc/sshd_config";
/// The product's `sftp-server`, the command `Subsystem sftp` names; the
/// server runs its own SFTP code for it.
pub const SFTP_SERVER: &str = "/QOpenSys/QIBM/ProdData/SC1/OpenSSH/libexec/sftp-server";

/// The host key files used when `HostKey` is not given.
#[must_use]
pub fn default_host_keys() -> Vec<String> {
    super::super::keys::KeyKind::ALL
        .iter()
        .map(|k| format!("{ETC_DIR}/{}", k.file_name()))
        .collect()
}

/// The configuration written when none exists.
#[must_use]
pub fn default_config() -> String {
    format!(
        "# QS/400 SSH server configuration.\n\
         # Directives the server does not support are accepted and ignored;\n\
         # see docs/SSHD.md for the list it honors.\n\
         \n\
         #Port 22\n\
         #ListenAddress 0.0.0.0\n\
         #ListenAddress ::\n\
         \n\
         #HostKey {ETC_DIR}/ssh_host_ed25519_key\n\
         #HostKey {ETC_DIR}/ssh_host_ecdsa_key\n\
         #HostKey {ETC_DIR}/ssh_host_rsa_key\n\
         \n\
         # QSECOFR is the root of this system.\n\
         #PermitRootLogin prohibit-password\n\
         #MaxAuthTries 6\n\
         #PubkeyAuthentication yes\n\
         #PasswordAuthentication yes\n\
         AuthorizedKeysFile .ssh/authorized_keys\n\
         \n\
         #AllowTcpForwarding no\n\
         #ClientAliveInterval 0\n\
         #Banner none\n\
         \n\
         Subsystem sftp {SFTP_SERVER}\n"
    )
}
