# The SSH server (`*SSHD`)

Crate `net/sshd`. An SSH-2 server with the SFTP subsystem, written for
QS/400 over pure-Rust cryptography, configured by an `sshd_config` at
the path IBM i uses, signing users on against the Phase 6 security
store, and running sessions as QSH in a QS/400 job. The TCP server
framework (`STRTCPSVR SERVER(*SSHD)`) starts it through
[`Daemon`](#integration) and wires the job services in; this crate
holds no job or store code of its own.

## Why not russh

The Phase 10 spec names `russh`. Every current release (0.53 to 0.65,
checked 2026-10-29) refuses to compile unless the `ring` or `aws-lc-rs`
feature is on (`compile_error!` in `lib.rs`), and both build C, which
`deny.toml` bans. The last release line without a C backend, 0.52.x,
resolves C-free but fails `cargo deny check advisories` four times:
RUSTSEC-2026-0153 and -0154 in russh itself (fixed only in 0.60.3+),
RUSTSEC-2026-0387 in `crypto-bigint` 0.5 and RUSTSEC-2023-0071 (Marvin)
in `rsa` 0.9, which has no fixed release. So the SSH-2 transport is
written here (`src/transport/`, about 700 lines) over `graviola =0.4.1`,
the pure-Rust primitives `net/tls` already uses, with `base64ct
=1.8.3` for base64. Nothing new enters the dependency graph.

## Protocol coverage

| Area | What is offered |
|---|---|
| Key exchange | `mlkem768x25519-sha256`, `curve25519-sha256` (and `@libssh.org`), `ecdh-sha2-nistp256`; strict key exchange (`kex-strict-s-v00@openssh.com`, the Terrapin fix); client-started re-keying |
| Host keys | `ssh-ed25519`, `ecdsa-sha2-nistp256`, `rsa-sha2-512`, `rsa-sha2-256` (no SHA-1 `ssh-rsa` signatures) |
| Ciphers | `aes256-gcm@openssh.com`, `aes128-gcm@openssh.com` only; no MAC is ever used, `hmac-sha2-256` is listed for clients that insist on a common one |
| Compression | `none` |
| Extensions | `EXT_INFO` with `server-sig-algs` after the first `NEWKEYS` when the client sends `ext-info-c` |
| User auth | `none` (lists the methods), `password`, `publickey` (query and signed); banner before the first answer |
| Channels | `session` (`pty-req`, `env`, `shell`, `exec`, `subsystem`, `window-change`, `signal`, `exit-status`, `exit-signal`), `direct-tcpip`, `forwarded-tcpip`; global `tcpip-forward`, `cancel-tcpip-forward`, `no-more-sessions@openssh.com`; `keepalive@openssh.com` answered with failure |

Not offered: `chacha20-poly1305@openssh.com` (needs raw ChaCha20 and
Poly1305, which graviola does not expose), CTR ciphers with HMAC,
`keyboard-interactive`, agent and X11 forwarding, `zlib`. Every OpenSSH
since 6.2, PuTTY since 0.76 and WinSCP take AES-GCM.

The server identifies as `SSH-2.0-QS400_V1R0M0` (the release from the
`release` crate).

## Configuration

The file is `/QOpenSys/QIBM/UserData/SC1/OpenSSH/etc/sshd_config`, the
path recorded on pub400 (V7R5M0: the product lives in
`/QOpenSys/QIBM/ProdData/SC1/OpenSSH`, whose `etc` is a link to the
`UserData` directory). At first start, if the file does not exist, the
server writes a short Quartz default (every directive commented out at
its default, `AuthorizedKeysFile .ssh/authorized_keys`, and `Subsystem
sftp` naming the product's `sftp-server`).

The parser is ours. Keywords are case-insensitive, `Keyword value` or
`Keyword=value`, double quotes group words. As in OpenSSH, the first
value of a single-valued keyword wins; list keywords accumulate.

| Directive | Honored as |
|---|---|
| `Port` | every port (default 22) |
| `ListenAddress` | `host`, `host:port`, `[v6]:port`; default every IPv4 and IPv6 address |
| `PermitRootLogin` | applies to `QSECOFR`: `yes`, `no`, `prohibit-password`/`without-password` (default), `forced-commands-only` |
| `PasswordAuthentication`, `PubkeyAuthentication` | default `yes` |
| `PermitEmptyPasswords` | default `no` |
| `AuthorizedKeysFile` | `%h`, `%u`, `%%`; relative to the home directory |
| `AllowUsers`, `DenyUsers` | `USER` or `USER@HOST` wildcard patterns, the host matched against the client address |
| `AllowGroups`, `DenyGroups` | matched against the group profile and supplemental groups |
| `Subsystem` | `sftp` is answered by the built-in server whatever command is named |
| `ClientAliveInterval`, `ClientAliveCountMax` | `keepalive@openssh.com` probes; times take `s m h d w` |
| `MaxAuthTries` | default 6 |
| `MaxSessions` | default 10 |
| `LoginGraceTime` | default 120 s |
| `Banner` | a file in the IFS, or `none` |
| `HostKey` | the key files; default the three below |
| `AllowTcpForwarding` | `no` (the QS/400 default; OpenSSH's is `yes`), `local`, `remote`, `yes`/`all` |
| `AcceptEnv` | names a client may set with `env` |

Directives known to have no effect here (`UsePAM`, `X11Forwarding`,
`GatewayPorts`, `PidFile` ...) and unknown ones are accepted, ignored
and logged with their line number. A `Match` line ends the parse with a
note: the block and the rest of the file are ignored.

## Host keys

`ssh_host_ed25519_key`, `ssh_host_ecdsa_key` (P-256) and
`ssh_host_rsa_key` (3072 bits) beside the configuration, each with its
`.pub`. A missing file is generated at start (mode 0600, the `.pub`
0644); a file that exists is never replaced, and one that cannot be
read stops the start rather than being overwritten. Files are written
as PKCS#8 PEM; PKCS#8, PKCS#1 (`RSA PRIVATE KEY`), SEC 1 (`EC PRIVATE
KEY`) and unencrypted OpenSSH-format Ed25519 and ECDSA keys are read,
so keys carried over from an IBM i system work except OpenSSH-format
RSA keys (see the open questions).

## Signing on

User names are folded to upper case and must be valid profile names.
The configuration's access rules come first (`DenyUsers`, `AllowUsers`,
`DenyGroups`, `AllowGroups`, then `PermitRootLogin` for `QSECOFR`); a
user they refuse fails exactly like a wrong password, without the
password being checked.

- **Passwords** go to `Accounts::check_password`. In production it is
  the storage service's sign-on (`sec::signon::sign_on` through the
  job's link, request 126), which counts the attempt against
  `QMAXSIGN`, applies `QMAXSGNACN` to the profile and writes the `PW`
  audit entry; the device name passed is `QSSHD`. An expired password
  is refused (no `PASSWD_CHANGEREQ` dialog).
- **Keys** are looked up in each `AuthorizedKeysFile`, read through the
  IFS as the server. Options honored: `from=` (pattern list against
  the client address), `command=` (forced; the client's command goes in
  `SSH_ORIGINAL_COMMAND`), `no-pty`, `no-port-forwarding`, `restrict`;
  `no-agent-forwarding`, `no-X11-forwarding`, `no-user-rc` and
  `environment=` are accepted without effect; a line with any other
  option is skipped, as OpenSSH does.
- **`MaxAuthTries`**: every failed `password` or `publickey` request
  counts (a key query for an unlisted key counts, a `none` request does
  not); at the limit the server disconnects with "Too many
  authentication failures", OpenSSH's text, which clients show.
- The user name may not change between requests on one connection.

## Sessions

A session channel becomes a `SessionRequest` (profile, `Shell`,
`Exec(command)` or `Subsystem`, the terminal, accepted `env` values,
both addresses) handed to `SessionHost::run` on a thread of its own,
with the channel as byte streams (`ChannelIo`: standard input, output,
error, and a control receiver for `window-change` and `signal`). The
host returns the exit status.

- `QshHost` runs QSH: `exec` as `qsh -c`, `shell` with a terminal as an
  interactive login shell, `shell` without one as a login shell reading
  its commands from standard input. The environment OpenSSH gives a
  session (`USER`, `LOGNAME`, `HOME`, `SHELL=/usr/bin/qsh`,
  `SSH_CLIENT`, `SSH_CONNECTION`, `TERM`, `SSH_ORIGINAL_COMMAND`) is in
  `SessionRequest::environment` for the job to set.
- With a terminal there is no kernel pty: `session::ldisc` does the
  canonical line discipline (echo, erase, kill, `^C`, `^D`, CR to NL,
  NL to CR NL on output). Commands that read standard input while
  running read the raw keystrokes.
- `FileTransfer` wraps any host and answers the `sftp` subsystem, an
  `exec` of `internal-sftp`, and `scp -t`/`scp -f` itself over the
  user's IFS; everything else goes to the wrapped host.
- Output to the client is held by the client's channel window: a
  session that writes faster than the client reads blocks in `write`.

## SFTP

Protocol version 3, the one OpenSSH speaks, over `IfsClient` with the
user's authority. Relative paths start at the home directory (`/` if it
does not exist). Every request is answered: `OPEN` (flags mapped to
`O_RDONLY/O_WRONLY/O_RDWR`, `O_CREAT`, `O_TRUNC`, `O_EXCL`,
`O_APPEND`), `CLOSE`, `READ` (at most 256 KiB), `WRITE`, `LSTAT`,
`FSTAT`, `STAT`, `SETSTAT`/`FSETSTAT` (size, permissions, owner and
group IDs, times), `OPENDIR`, `READDIR` (100 entries per answer with an
`ls -l` line), `REMOVE`, `MKDIR`, `RMDIR`, `REALPATH` (lexical),
`RENAME` (fails if the target exists), `READLINK`, `SYMLINK` (OpenSSH's
argument order: target first). Extensions: `posix-rename@openssh.com`,
`hardlink@openssh.com`, `fsync@openssh.com`.

Mapping notes:

- IFS errors map to `NO_SUCH_FILE` (`ENOENT`, `ENOTDIR`),
  `PERMISSION_DENIED` (`EACCES`, `EPERM`), `OP_UNSUPPORTED` (`ENOTSUP`,
  `ENOSYS`) and `FAILURE` for the rest; the status text is the errno's
  Quartz text.
- Type bits other than directory, link, FIFO, socket and devices
  (`/QSYS.LIB` objects) are shown as regular files.
- `/QSYS.LIB` members are moved as their records back to back (no
  conversion, no line ends). `stat` reports IBM's size, which counts two
  line-end bytes per source record, so it is larger than what a read
  returns; clients read to end of file and are not affected.
- The `ls -l` line shows the owner and group profile names.

## scp

Current OpenSSH `scp` uses SFTP. The legacy protocol (`scp -O`, older
clients) is answered for `scp -t` (receive: `C`, `D`, `E`, `T` records;
`-r`, `-p`, `-d`) and `scp -f` (send, recursive with `-r`, times with
`-p`), errors as `\x01scp: ...` lines and exit status 1.

## Forwarding

Off by default. `AllowTcpForwarding` turns on local (`direct-tcpip`)
and/or remote (`tcpip-forward`) forwarding; a key with
`no-port-forwarding` or `restrict` never forwards. A remote forward
also asks the `ADDTCPPORT` restrictions (`sockets::PortRules`,
`may_bind(IPPROTO_TCP, port, user)`) before listening. Connections and
listeners come from the `Forwarder` trait; `TcpForwarder` uses the
kernel's TCP. When the forwarded connection's far end closes, the
channel is closed (no half-close).

## Integration

The framework calls, for `STRTCPSVR SERVER(*SSHD)`:

```rust
let deps = sshd::Deps {
    accounts,   // Arc<dyn sshd::auth::Accounts>: lookup + check_password (the link's sign_on)
    host,       // Arc<dyn sshd::session::SessionHost>: usually FileTransfer::new(QshHost::new(job_sys), fs.clone())
    fs,         // Arc<dyn sshd::io::FileSystem>: system() and user(account) IfsClients (RemoteIfs over a link)
    ports,      // Arc<dyn sockets::PortRules + Send + Sync>: the ADDTCPPORT table
    forwarder,  // Arc<dyn sshd::conn::Forwarder>: sshd::conn::TcpForwarder
};
let server = Arc::new(sshd::Server::start(sshd::config::paths::CONFIG_FILE, deps)?);
let daemon = sshd::Daemon::bind(server)?;      // binds ListenAddress x Port
let stop = daemon.stopper();                    // ENDTCPSVR: stop.store(true)
std::thread::spawn(move || daemon.run());
```

`Server::serve(read, write, peer, local)` serves one connection over any
byte stream and `Server::serve_tcp(&TcpStream)` one accepted socket, for
a framework that accepts connections itself (prestart jobs, descriptor
passing). Still to wire: the `Accounts` over the job's link (`sign_on`
plus a profile read for `HOMEDIR`, `GRPPRF`, `SUPGRPPRF`), the job a
session runs in (IBM shows sshd as a `QP0ZSPWT` prestart job in
`QUSRWRK` running as `QUSER`, sessions as spawned jobs running as the
user), and a `JobSys` giving `QshHost` that job's `qsh::Sys`.

## Tests

`cargo test -p sshd`: unit tests for every codec and parser, and two
end-to-end suites over an in-memory stream with the crate's own client
(`sshd::client`) against a security store (profiles, `QMAXSIGN`, `PW`
audit) and an IFS with `/QSYS.LIB`: every key exchange method with every
host key type, password and key sign-on, the access rules,
`PermitRootLogin`, `MaxAuthTries`, `QMAXSIGN`, exec, a terminal shell,
SFTP on `/home`, `/QOpenSys` and `/QSYS.LIB`, the legacy scp both ways,
and forwarding (refused, allowed, held to a port restriction).
`cargo test -p sshd --test openssh -- --ignored` runs the real OpenSSH
`ssh`, `sftp` and `scp` (SFTP and `-O`) against the daemon on loopback.
