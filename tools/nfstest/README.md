# nfstest

The NFS test client for QS/400's NFS server (`docs/NFS.md` section 11):
scenarios a client runs against a server, over loopback or through a
forwarded port, each printing `ok NAME` or `fail NAME: reason` (and
`skip NAME: reason` when it cannot run against the target).

## Independent of the server

nfstest must not share a codec with the server it tests, or a misreading
of an RFC could hide in both. Nothing in its binary depends on `net/nfs`,
`net/xdrgen` or their generated types; the server appears only in the
tests (dev-dependencies), to be brought up on loopback.

| Part | Where it comes from |
|---|---|
| XDR primitives for the crates below | `onc-xdr` 1.3.1 (Apache-2.0, the NFSWolf project) |
| RPC message header (`rpc_msg`, `opaque_auth`, `AUTH_SYS` body, reply and refusal bodies) | `onc-rpc-client` 1.3.1 (Apache-2.0) |
| portmap version 2 (`mapping`, `pmaplist`) | `onc-rpcbind` 1.3.1 (Apache-2.0) |
| MOUNT version 3 (`dirpath`, `mountres3`, `mountlist`, `exports`) | `nfs-mount` 1.3.1 (Apache-2.0) |
| NFS version 3, all 22 procedures' arguments and results | `nfs-v3` 1.3.1 (Apache-2.0) |
| Record marking, sockets, XIDs, retransmission | hand-written, `src/rpc/` |
| An XDR reader and writer for what follows | hand-written, `src/wire/xdr.rs` |
| NLM version 4 and NSM version 1 | hand-written from XNFS (RFC 1813 appendix II), `src/wire/nlm.rs` |
| NFS version 4.0 and 4.1 `COMPOUND`, the operations used, `fattr4` | hand-written from RFC 7530, 7531, 8881, 5662, `src/wire/v4.rs`, `src/wire/v4attr.rs` |
| The callback portmapper and NLM `GRANTED` service | hand-written, `src/callback.rs` (RPC header from `onc-rpc-client`) |

All pinned with `=`, pure Rust (the graph has `tokio`, which the
`onc-rpc-client` crate requires; nfstest does not start a runtime). No
crate qualified for NLM, NSM or NFS version 4: `nfs-v4` 1.3.1 has no 4.1
sessions (`EXCHANGE_ID` is an opaque payload) and `nfs-rs` calls
`getuid` unconditionally (no Windows build) and hides the procedures;
`nfs3_client`/`nfs3_types` are Unlicense.

nfstest sits outside the workspace with its own `Cargo.lock` (the root
`Cargo.toml`'s `exclude`), as `tools/smbtest` does; `cargo run -p build --
deny` checks its graph (`tools/build/src/product.rs`).

## Command line

```
nfstest HOST EXPORT SCRATCH [options]          run the scenarios
nfstest probe HOST [--pmap-port N] [--port-map G=H ...] [EXPORT]
nfstest bench HOST EXPORT SCRATCH [--mib N] [--bs N] [options]
nfstest --list
```

- `EXPORT` is the export's path (for version 4, its path in the pseudo
  file system); `SCRATCH` a directory under it the scenarios write in,
  made when missing. Each scenario works in `SCRATCH/<scenario>-<pid>`
  and removes it at the end.
- `--vers 3|4.0|4.1` (default 3) chooses the run: 3 is RPC, MOUNT, NFS
  version 3, NLM and NSM; 4.0 and 4.1 their version 4 scenarios.
  `--only A,B` runs the named scenarios whatever the version.
- `--proto tcp|udp` (default tcp).
- Ports: `--pmap-port N` (default 111), `--port N` (NFS), `--mountport`,
  `--nlmport`, `--nsmport`. A port not given is asked of the binder
  (portmap `DUMP` over TCP, then UDP). `--port-map GUEST=HOST`
  (repeatable; `GUEST:HOST` too) translates the binder's answers, for a
  guest behind QEMU's user-mode forwards.
- `--uid N --gid N` (default 0 0): the `AUTH_SYS` credential.
  `--expect-root`: UID 0 is not squashed by this export.
- `--callbacks`: run `nlm-blocked-granted`, which needs the server to call
  this client back: nfstest binds a portmapper on `--cb-pmap-port`
  (default 111) and a lock manager on `--cb-nlm-port` (default any) at
  `--cb-host` (default 127.0.0.1) and names the waiting lock by that
  address. It cannot work through one-way port forwards.
- `--expect-grace`: run `nlm-grace` right after the server starts.
- `--timeout SECS` (5) per reply; `--large-mib N` (16) the large file;
  `--entries N` (1500) the large directory.
- The exit code is the number of failed scenarios (at most 100); 101 is a
  usage error. `probe` prints the binder's table as `rpcinfo -p` does
  (with `(host port N)` after a port the map changed) and the export list
  as `showmount -e` does. `bench` prints `bytes`, `block`, `write MiB/s`
  (to the `COMMIT`) and `read MiB/s`, after comparing checksums.

## Scenarios

`nfstest --list` prints them. Version 3 run:

- RPC: `null-pmap` (versions 2, 3, 4), `null-nfs`, `null-mount` (1, 3),
  `null-nlm` (1, 3, 4), `null-nsm`; `rpc-prog-unavail`;
  `rpc-nfs-v2-mismatch` (`PROG_MISMATCH` 3..3 or 3..4, which it prints);
  `rpc-proc-unavail`; `rpc-garbage-args` (a 255-byte handle, a short body,
  a bool of 7); `rpc-version-mismatch` (RPC version 3); `rpc-oversize-record`
  (2 MiB of fragments: no reply, the connection closed, a new one served);
  `rpc-gss-refused` (`AUTH_ERROR`); `auth-none`, `auth-root-squash`,
  `auth-unknown-uid` (who owns what each creates); `dup-request` (`RENAME`
  and `REMOVE` sent twice with one XID get the same answer).
- MOUNT: `mount-cycle` (`MNT`, `DUMP`, `EXPORT`, `UMNT`), `mount-not-exported`.
- Attributes: `getattr-setattr` (mode, size, both times, server time),
  `setattr-guard`, `lookup-dots` (`.` and `..` at the export's root and
  below), `access-bits`, `fsinfo-fsstat-pathconf`, `stale-handle`,
  `bad-handle`.
- Files: `create-modes`, `create-exclusive` (the same verifier again
  succeeds, another is `EXIST`), `write-unstable-commit` (verifier stable),
  `write-file-sync`, `read-offsets-eof`, `large-file` (16 MiB in the
  server's preferred sizes, FNV-1a compared), `append`, `truncate`,
  `symlink-readlink`, `mknod-fifo`, `mknod-device` (character device and
  socket refused, `NF3REG` `BADTYPE`), `hard-link`, `remove`, `udp-core`
  (`MNT` to `RMDIR` over UDP).
- Directories: `mkdir-rmdir`, `rename-over`, `rename-across`,
  `rename-dir-into-itself`, `readdir-paging` and `readdirplus-paging`
  (1,500 entries in small replies: each name once, none missing),
  `readdir-bad-cookie`.
- Names: `name-255`, `name-256` (`NAMETOOLONG`), `name-slash`,
  `name-non-utf8` (reports what happens).
- NLM and NSM: `nlm-test-lock-unlock`, `nlm-conflict` (two hosts, two
  processes, shared and exclusive), `nlm-blocked-granted` (`--callbacks`),
  `nlm-cancel`, `nlm-free-all`, `nlm-share`, `nlm-grace`
  (`--expect-grace`), `nsm-stat`.

Version 4: `v4-null`, `v4-minor2-refused`, `v4-op-illegal`; 4.0
`v40-setclientid-renew`, `v40-lookup-export` (down the pseudo file
system, the required attributes decoded), `v40-open-write-read` (`OPEN`
create and `OPEN_CONFIRM`, `WRITE`, `COMMIT`, `READ`, `CLOSE`),
`v40-readdir`, `v40-rename-remove`, `v40-setattr`, `v40-locks` (`LOCK`,
`LOCKT`, `LOCKU`, two owners), `v40-share-deny`, `v40-sequence-refused`;
4.1 `v41-exchange-session` (and `DESTROY_SESSION`/`DESTROY_CLIENTID`, the
old session then `BADSESSION`), the same file scenarios as `v41-*` with
`CLAIM_NULL`, `v41-slot-replay` (the cached reply), `v41-misordered-sequence`,
`v41-no-sequence-refused`, `v41-layoutget-refused` (`NFS4ERR_NOTSUPP`),
`v41-acl-not-returned`.

## Tests

`cargo test --manifest-path tools/nfstest/Cargo.toml`:

- `tests/loopback.rs`: QS/400's server (`nfs::server::run`: a binder over
  TCP and UDP, `mountd`, `nfsd`, `nlmd`, `nsmd`) over an IFS in memory
  with one export, `/home/exp`, on loopback; every version 3 scenario
  through `cli::run` with `--callbacks`; `probe`; `bench` of 8 MiB;
  `nlm-grace` against a server in its grace period. Scenarios that fail
  against the server on main are listed in `KNOWN` with the finding.
- `tests/v4_scripted.rs`: the version 4 scenarios that need no file data
  against a scripted responder (`tests/v4fake/`, written from the RFCs
  for the tests); that a version 3 server refuses version 4; and every
  4.0 and 4.1 scenario against QS/400's server (`nfsd4`).

## Interop

`tests/interop.rs`, all ignored, for a Linux host (this project's
Windows build PC has neither):

- `NFSTEST_LINUX=host:/export` (and `NFSTEST_LINUX_SCRATCH`, `NFSTEST_UID`,
  `NFSTEST_GID`): every scenario, versions 3, 4.0 and 4.1, against a Linux
  `nfsd` export, to check nfstest itself against a second server.
  `cargo test --manifest-path tools/nfstest/Cargo.toml --test interop -- --ignored`
- `kernel_client_mounts_the_server` (root, `mount.nfs`): the Linux kernel
  client mounts QS/400's server on loopback with explicit ports and
  `nolock`, writes, reads back, lists, unmounts.

## In the QEMU suite

The harness builds the binary (`cargo build --release --locked
--manifest-path tools/nfstest/Cargo.toml`, or takes `QS400_NFSTEST`),
forwards the guest's 111 and 2049, runs `nfstest probe 127.0.0.1
--pmap-port <host 111>` to learn the registered ports of MOUNT, NLM and
NSM, adds a forward for each (QMP `hostfwd_add`), then runs
`nfstest 127.0.0.1 <export> nfstest --pmap-port <host 111> --port-map
<guest>=<host> ...` (or the explicit `--port`/`--mountport`/`--nlmport`/
`--nsmport`) and `--vers 4.0`/`4.1` with `--port` alone. `--callbacks`
is not used through forwards. The 1 GB run is `nfstest bench ... --mib
1024`.

## Findings

What the loopback run found in the server (in `KNOWN` until fixed;
none is now):

- `nlm-share` (fixed): an NLM `SHARE` with deny-write of a file the same client
  has just written through NFS version 3 is `NLM4_DENIED`, and stays
  denied while the writing connection is idle; one more NFS call on that
  connection and it is granted. The `nfsd` thread's cached write
  descriptor (`server/ifsvfs/fdcache.rs`) counts as an open with no share
  mode, and its idle time is only checked on the thread's next call
  (`IfsVfs::sweep`). The same descriptor would refuse a version 4 `OPEN`
  with `OPEN4_SHARE_DENY_WRITE` and a local job's `open` with
  `O_SHARE_NONE`. Now a share refusal closes the idle threads' kept
  descriptors of the object across the job (`server/ifsvfs/jobfds.rs`)
  and the open is tried once more (`net/nfs/tests/srv3_fdshare.rs`); a
  local job's open still waits for the job's next call (docs/NFS.md 5.2).
