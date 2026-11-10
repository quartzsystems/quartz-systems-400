# QS/400 patches to rust-smb-server

This directory is [rust-smb-server](https://github.com/paltaio/rust-smb-server)
0.10.0 from crates.io (MIT, `LICENSE`, copyright Palta.io), committed
unmodified in `8c1a0bdc4` ("Vendor rust-smb-server 0.10.0 (MIT)
unmodified"). It is excluded from the workspace (`Cargo.toml` `exclude`),
so the workspace's lints do not apply to it, and is used by `net/netserver`
with `default-features = false` (no `localfs`, no `discovery`).

Each change is marked `QS/400:` in the source. `git diff 8c1a0bdc4 --
net/smb-server` shows all of them. They are candidates for upstream.

| Patch | Files | Why |
|---|---|---|
| Logon verifier | `src/verifier.rs` (new), `src/proto/auth/ntlm.rs` (`authenticate_with`, `local_session_base_key`), `src/handlers/session_setup.rs`, `src/builder.rs` (`logon_verifier`), `src/server.rs` (`ServerConfig::verifier`), `src/lib.rs` | The server keeps its users as NT hashes in memory. On QS/400 the NT hash stays in the storage service (`docs/NETSERVER.md` section 5), so the server hands the NTLMv2 response to a `LogonVerifier`, which returns the session base key and the profile name. Without a verifier the server behaves as upstream |
| Any-user grant | `src/builder.rs` (`Share::any_user`), `src/server.rs` (`ShareAcl::authorize`, `add_share`), `src/verifier.rs` (`ANY_USER`) | On QS/400 every signed-on profile may connect to a share and the IFS authorities decide the rest; upstream grants shares only to named users |
| One connection | `src/server.rs` (`SmbServer::serve_connection`) | `QZLSSERVER` accepts and hands each connection to its own `QZLSFILE` job (`docs/NETSERVER.md` section 3), which serves it with this |
| Guest logons and the client address for the verifier | `src/verifier.rs` (`Logon::guest`, `LogonVerifier::verify_from`), `src/proto/auth/ntlm.rs` (`authenticate_with` takes `Option<Option<key>>`), `src/handlers/session_setup.rs`, `src/conn/state.rs` (`Session::keyless`), `src/dispatch.rs`, `src/handlers/tree_connect.rs` | NetServer's guest profile: an unknown user name signs on as the guest profile, an `IS_GUEST` session named for the profile with no key, so nothing is signed or encrypted and its MIC is not checked; the share grants apply to the profile; `verify_from` gives the verifier the connection's address for the audit entry |
| Share modes to the backend | `src/backend.rs` (`OpenOptions::share_access`), `src/handlers/create.rs`, the `OpenOptions` literals in `src/` and `tests/files_create_action.rs` | one `QZLSFILE` job per client connection: share modes the server keeps in its own table do not reach other jobs, so the backend also opens the IFS descriptor with the share mode |
| Byte-range locks to the backend | `src/backend.rs` (`Handle::lock_range`, `Handle::unlock_range`), `src/handlers/lock.rs` | as above, for locks: after the server grants a lock in its table the backend takes it in the IFS too, and a refusal there undoes it |
| Minimum dialect | `src/builder.rs` (`min_dialect`), `src/server.rs` (`ServerConfig::min_dialect`), `src/handlers/negotiate.rs` (`select_dialect`), `src/handlers/ioctl.rs`, `src/dispatch.rs` | NetServer's SMB2 minimum dialect attribute |
| Print shares | `src/builder.rs` (`Share::print`), `src/server.rs` (`ShareBindings::is_print`, `print_files`), `src/handlers/tree_connect.rs` (share type PRINT), `src/handlers/create.rs` (every CREATE makes `PRINTnnnnnnnn`), `src/rpc/srvsvc.rs` (`STYPE_PRINTQ`) | NetServer print shares; upstream has disk shares and IPC$ only |
| Durable opens across `serve_connection` | `src/server.rs` (`serve_connection` keeps them; `disconnected_opens`, `client_guids`, `finish`), `src/files/durable.rs` (`disconnected`, `disconnected_clients`) | `QZLSSERVER` routes a reconnecting client back to the job holding its durable opens (`docs/NETSERVER.md` section 3); the job ends once it serves no connection and keeps no open |
| Domain and server comment | `src/builder.rs` (`domain`, `comment`), `src/server.rs`, `src/handlers/session_setup.rs` (the CHALLENGE's NetBIOS domain), `src/rpc/srvsvc.rs` (SERVER_INFO_101's comment) | NetServer's domain (workgroup) and text attributes |
| NetBIOS keepalives | `src/conn/reader.rs` | on port 139 clients send SESSION KEEP ALIVE packets between messages; upstream closed the connection on them |
| Fuzzing feature | `Cargo.toml` (`fuzzing`), every `cfg(fuzzing)` | `smb_server::fuzzing` for `tools/fuzz`, which builds without `--cfg fuzzing` |
| spoolss | `src/print.rs` (new: `PrintService`, `PrintJob`, `PrinterInfo`, `DriverFiles`, `DocInfo`, `JobInfo`, `PrintError`), `src/rpc/spoolss/` (new: the MS-RPRN calls, custom-marshaled INFO structures, forms, handles, a print service in memory for tests and fuzzing), `src/rpc/mod.rs` (`Interface`), `src/rpc/pdu.rs` (`SPOOLSS`), `src/rpc/association.rs` (the interface a pipe binds, its request size limit), `src/rpc/ndr.rs` (u16, handles, byte arrays, longer strings), `src/rpc/pipe.rs` (the pipe's interface and handles), `src/handlers/pipe.rs` (CREATE and FSCTL_PIPE_WAIT of `spoolss`), `src/builder.rs` (`print_service`), `src/server.rs` (`ServerConfig::print_service`), `src/lib.rs` (`print`, `fuzzing::spoolss_request`) | Point and print (`docs/NETSERVER.md` section 7): `\pipe\spoolss` on IPC$ serves the print spooler calls Windows uses to connect a printer and print through it, over a `PrintService` the application implements. Without one the pipe does not exist, as upstream |
| Tests | `tests/qs400_hooks.rs`, `tests/qs400_patches.rs`, `tests/qs400_spoolss.rs`, `Cargo.toml` (`[[test]]` entries) | the hooks and patches above; spoolss over a connection |

## Tests

`cargo test --no-fail-fast` in this directory (target dir outside the tree): 1156 pass
and one upstream Windows-host test fails
(`files_eas_windows::listings_report_reparse_points_as_their_opens_do`:
its `mklink` call fails under Git Bash), with and without the patches.

## Updating

Copy the new release over this directory, keep this file and the
`QS/400:` changes, run the tests, and update the version in
`net/netserver/Cargo.toml` and above.
