# Phase 10: IFS, network, servers, QSH

Phase 10 gives QS/400 a file system beside the object store and puts it
on the network: the integrated file system on xfs with `/QSYS.LIB`, its
commands and ILE APIs; communications objects and TCP/IP configuration;
sockets, the resolver, TLS and the certificate store; the TCP servers
(telnet, FTP, SSH, NTP, LPD, the HTTP admin page, DDM/DRDA and the host
servers ACS and JTOpen use); network printers; and QSH with its
utilities and PASE.

Each area has its own document; this file is the inventory, the
deviations, what is blocked or deferred, the bench and the acceptance
state.

| Area | Document | Crates |
|---|---|---|
| IFS | `IFS.md` | `ifs/ifs`, `ifs/xfsfmt`, `ifs/qsyslib`, `ifs/ifscmd`, `devtools/edtf` |
| Communications objects, TCP/IP | `TCPIP.md` | `net/netlink`, `net/cmncfg`, `net/tcpcfg`, `net/nettools` |
| Sockets, TLS, certificates | `TCPIP.md`, `CERTS.md` | `net/sockets`, `net/tls`, `net/certs` |
| TCP servers | `SERVERS.md`, `FTP.md`, `SSHD.md`, `DDM.md` | `net/servers`, `net/ftp`, `net/sshd`, `net/ntp`, `net/lpd`, `net/mail`, `net/ddm` |
| Host servers, exit points | `HOSTSVR.md` | `net/hostsvr`, `net/hostserver`, `net/regfac` |
| Printers | `PRINT.md` | `work/print` |
| QSH, PASE | `QSH.md` | `qsh/qsh`, `qsh/qsh-utils` |
| Open questions | `NET_OPEN_QUESTIONS.md` | |

## Oracles

| System | Release | Reached through | Used for |
|---|---|---|---|
| Cody's box (primary) | V7R1M0 | JTOpen host servers only (449, 8470-8476) | the host-server traces and the JTOpen suite's oracle runs (2026-10-24), API headers and exports, the `oracle/net/primary/` capture and the fetch timing (2026-10-30; it was down from 10-24 to 10-30); read-only for every network setting |
| pub400.com | V7R5M0 | telnet 23, FTP 21, host servers | screen goldens (CFGTCP, NETSTAT, WRKLNK, EDTF, prompts), the FTP transcripts, DDM traces, message captures; SSH is not reachable from this PC |

Every capture lives under `tools/conformance/`; probes write the pub400
account as `{user}`.

## Inventory

| Commit | What |
|---|---|
| `0c1148e1` | the netlink layer, kernel options, CMN resources |
| `ab7ee6d1` | the IFS: namespace, metadata, authority, locks, CCSIDs |
| `8110026d` | QSH: the language, builtins and sessions |
| `bc584e1a` | host servers: the datastream codec, traces, the JTOpen suite |
| `2746fb36` | the QS400-IFS partition, the xfs formatter, the IPL mount |
| `404f4acd` | conformance: Phase 10 goldens, facts, FTP transcripts |
| `bef24777` | the certificate store, the TLS engine, the GSKit layer |
| `450828f9`, `55691727` | QSH utilities (73 of the 163 `/usr/bin` entries) |
| `48d7111a` | the job's descriptor table, socket and resolver APIs |
| `35f32cca` | the IFS in the storage service, the `/QSYS.LIB` view |
| `94f82b77` | LAN printers, remote output queues, the host print transform |
| `1b2e720b` | the registration facility, exit points, `WRKREGINF` |
| `bc85493f` | line descriptions, `VRYCFG`, `WRKCFGSTS` |
| `c2acb965` | TCP/IP configuration, its commands, the CFGTCP panels |
| `ee6214f4` | DDM/DRDA: codec, server, requester, the RDB directory |
| `46ed508c` | the ILE IFS APIs, `SRCSTMF`, stream-file `/COPY` |
| `ce07c4f2` | the IFS commands, `WRKLNK`, `EDTF`, `DSPF`, stream copies |
| `82ee33e0` | the TCP server framework, telnet under it, the HTTP admin page |
| `608d4dbf` | the SSH-2 server with SFTP and scp over the IFS |
| `d2ba057e` | the host server daemons, prestart jobs, JTOpen under QEMU |
| `c6c02fd7` | `NETSTAT`, `PING`, `TRACEROUTE`, the communications trace |
| `d42a765f` | SNTP, LPD/LPR, `SNDSMTPEMM` |
| `b4e1724b` | the FTP server, the `FTP` command, `CHGFTPA`, the replay kit |
| `d00e7d77` | the `QS*` certificate commands, server certificates, expiry |
| `f917ac0f` | `cargo deny` fix, shell-utility bans, the Phase 10 tripwire |

The QEMU suite grew from 55 steps to the ones below (Phase 10 adds 59
to 70; step 65 is QSH):

| Step | Proves |
|---|---|
| 59 | the IFS partition mounted, its capacity |
| 60 | `CRTLINETH` over the virtio port, `VRYCFG`, carrier loss |
| 61 | TCP/IP configuration applied to the kernel, a host port forwarded to a virtual IP |
| 62 | `ENDTCP` and `STRTCP` take the addresses and telnet down and back |
| 63 | the IFS commands: `MD`, a member to a stream file and back, `WRKLNK`, `DSPF`, `DEL`, `RD` |
| 64 | `STRTCPSVR`/`ENDTCPSVR` of `*HTTP` and `*TELNET`, `CHGTELNA` |
| 65 | QSH: environment variables, `QSH CMD`, a batch QSH job, PASE |
| 66 | `PING`, `TRACEROUTE`, `NETSTAT`, a communications trace |
| 67 | the FTP server: sign-on, members, stream files, `RCMD`, the scripted `FTP` command |
| 68 | the host server daemons and prestart jobs, JTOpen `CommandCall` and JDBC |
| 69 | `*LPD` receiving an LPR job into an output queue, `*NTP` answering SNTP |
| 70 | a certificate assigned to `QIBM_HTTP_SERVER_ADMIN` served on 2005 |

## Deviations

- **Byte order in job storage.** Job storage is big-endian like IBM i's,
  so `htonl`, `htons`, `ntohl` and `ntohs` are identities and socket
  structures are laid out as IBM i lays them out. Linux system calls get
  host-order values from the sockets layer (`net/sockets`).
- **No russh.** The spec named russh for the SSH server. Every current
  russh needs `ring` or `aws-lc-rs` (both build C), and the last version
  that does not (0.52.1) fails `cargo deny`'s advisories. The SSH-2
  transport is ours (`net/sshd/src/transport/`) over `graviola`, the
  pure-Rust crypto `net/tls` already uses. It offers ML-KEM-768/X25519,
  curve25519 and P-256 key exchange with strict KEX, ed25519, P-256 and
  RSA-SHA2 host keys, and AES-GCM; graviola has no raw ChaCha20-Poly1305,
  so `chacha20-poly1305@openssh.com` is not offered (PuTTY before 0.76
  cannot connect).
- **New `unsafe` sites.** Two files join CLAUDE.md's list:
  `net/nettools/src/sys/raw.rs` (`setsockopt` of an int, `recvmsg` with
  control data, `AF_PACKET`) and `work/qsjob/src/platform/clock.rs`
  (`adjtimex`, which musl's libc binding lacks). Each block has a
  `SAFETY` comment.
- **IBM text kept as interface.** Three `db2` output lines
  (`DB20000I ...`, `RECORD(S) SELECTED.`, `**** CLI ERROR *****`) and
  the FTP `SYST` sentence naming OS/400 are printed as IBM prints them
  because scripts and clients test for them;
  `tools/conformance/tests/phase10_hygiene.rs` lists them and fails if
  they are used anywhere else.
- **Job lookup by bare name.** `ENDJOB JOB(NAME)` and the server
  framework find a job by its name alone only when exactly one active
  job has it.

## JTOpen

JTOpen (`jt400.jar`, IBM Public License 1.0) is a test-time client
only: the suite's Java programs under `tools/conformance/jtopen/src/`
are ours and run against a downloaded jar outside the tree. No JTOpen
source is in QS/400 or was copied into it; protocol facts come from
our own traces of the oracle (`tools/conformance/hostsvr/`).

## Blocked

Each item is marked `BLOCKED(phase10)` where it lives.

| Item | Where | Waits for |
|---|---|---|
| RCLSTG migration of an IFS into a disk with no tail space | `lic/slic/src/ipl/ifs.rs` | shrinking ASP 1 |
| DDM files open the remote file | `work/qsjob/src/command/ddm.rs` | `*FILE` objects of attribute DDM in the database |
| `*DDM` server on 446 | `net/servers` registry | `SqlBackend`/`FileBackend`/`Authenticator` over the storage service |
| Host servers read a spooled file's data | `work/qsjob/src/work/hostsvr/job.rs` | a storage request that returns spooled-file bytes |
| Telnet `INIT0100` automatic sign-on | `work/qsjob/src/work/telnet.rs` | a storage request that signs on without a password |
| `SNDTCPSPLF WSCST(name)` | `work/qsjob/src/command/netcmd.rs` | reading `*WSCST` in the job |
| SMTP `AUTH` credentials | `net/servers/src/attrs/smtp.rs` | a command that sets them and storage only the mail job reads |

Cleared during the phase: the IPP printer TLS connector, the FTP server's
and client's certificates, the SMTP relay check and TCP/IP line state
(all through the `*SYSTEM` store request `cert_app` and `*LIND` objects).
DHCP is not built (`*IP4DHCP` interfaces are refused).

## Deferred

| Item | To |
|---|---|
| `SAV`/`RST` media | Phase 11 (they end with `CPF3700` today) |
| `INFOSTMF` PCML output of the RPG compiler | Phase 11 (`MIGRATE(phase11)`) |
| the full HTTP server and its administration UI, Navigator | after Phase 12 |
| `TRCCNN`, `TRCTCPAPP`; NETSTAT sort, subset and multicast groups | later |
| an interactive `FTP` client display (scripted and batch use work) | later |
| user stored procedures (`CALL`) in SQL, so in JDBC | the SQL engine |
| DHCP and IP filters | later |

## Bench

Run 2026-10-30 on the development PC: Intel Core i9-12900K (16 cores,
24 threads), 32 GB, Windows 11 IoT Enterprise LTSC 2024, rustc 1.98.0,
release profile. The LinuxBackend row ran in WSL 2 (Ubuntu, kernel
6.18.33.2-microsoft-standard-WSL2) on a 2 GiB xfs loop image. Each
figure is the middle of three runs; ranges in brackets.

| Bench | Result |
|---|---|
| FTP `STOR`, 64 MiB stream file, binary, passive, loopback | 215 MiB/s [215-352] |
| FTP `RETR`, the same file | 735 MiB/s [598-745] (reading the file before `150`: 0.06 s; sending: 0.03 s) |
| SFTP put, 64 MiB, loopback TCP, AES-GCM | 265 MiB/s [255-303] |
| SFTP get, the same file | 361 MiB/s [360-392] |
| SSH key exchange + password sign-on + SFTP start | 0.017 s |
| Host server SQL fetch, 10,000 rows, loopback TCP | 288,000 rows/s [276,000-291,000], 435 rows per reply |
| IFS over MemBackend: create + write 4 KiB + close | 75,500 files/s [73,200-76,800] |
| IFS over MemBackend: `stat` | 190,000 ops/s [185,000-190,000] |
| IFS over MemBackend: `unlink` | 147,500 ops/s [147,500-152,800] |
| IFS over LinuxBackend (xfs, WSL): create + write 4 KiB + close | 18,100 files/s (one run) |
| IFS over LinuxBackend (xfs, WSL): `stat` | 53,000 ops/s (one run) |
| IFS over LinuxBackend (xfs, WSL): `unlink` | 37,600 ops/s (one run) |

How each is measured:

- FTP (`net/ftp/tests/bench.rs`): the server (`ftp::server::serve` over
  `TcpNet`) with a `MemHost` (the IFS on a `MemBackend`) on loopback; a
  bare client signs on, `TYPE I`, `PASV`, `STOR` of 64 MiB in 1 MiB
  writes, then `RETR`. Each time runs from the command to the `226`.
  The server reads a whole file before it sends, so `RETR` is the IFS
  read plus the transfer. The client closes the data connection at end
  of data as real clients do (the server waits up to 2 s for that
  before `226`).
- SFTP (`net/sshd/tests/bench.rs`): the server's `serve_tcp` on
  loopback, the crate's own client over the same IFS in memory as the
  sshd tests. The client keeps one 32 KiB request in flight, so this is
  a lower bound for a client that pipelines (OpenSSH's `sftp` keeps 64
  in flight).
- Host server SQL (`net/hostserver/tests/db_bench.rs`): the database
  server over loopback TCP on the Phase 3 SQL engine with a store in
  memory, a table of `INTEGER, CHAR(10), DECIMAL(11,2), VARCHAR(50)`;
  the client sends what `JTOpen` sends for a query: seed exchange,
  start server, prepare and describe (`0x1803`, UTF-16 text in
  `0x3831`), open, describe and fetch (`0x180E`), fetches (`0x180B`)
  with a 32 KiB buffer (`0x3834`, `JTOpen`'s default block size) to end
  of data, close. Replies are not RLE-compressed. The time covers
  prepare to close; loading the table is not timed.
- IFS (`ifs/ifs/tests/bench.rs`): `Ifs` operations as user `BOB`
  (resolution, authority, the metadata record) on 20,000 files in one
  directory the user owns. On Linux, `ifs_ops_linux` runs the same over
  `LinuxBackend` in `QS400_IFS_TEST_DIR` (root, a file system with
  `trusted.` attributes); the run above used a script modelled on
  `ifs/ifs/tests/linux-xfs.sh`. In the same WSL run MemBackend measured
  71,900 creates/s, 230,000 stats/s and 200,600 unlinks/s, so the
  xfs-backed figures are about a quarter of the in-memory ones on the
  same kernel.

Commands:

```
cargo test --release -p ftp --test bench -- --ignored --nocapture        # QS400_FTP_BENCH_MIB
cargo test --release -p sshd --test bench -- --ignored --nocapture       # QS400_SFTP_BENCH_MIB
cargo test --release -p hostserver --test db_bench -- --ignored --nocapture  # QS400_HS_BENCH_ROWS
cargo test --release -p ifs --test bench -- --ignored --nocapture        # QS400_IFS_BENCH_FILES
```

Oracle comparison: on 2026-10-30 JTOpen's JDBC driver (block size 32
KiB, prefetch) fetched 10,000 rows of `QSYS2.SYSCOLUMNS` (four columns)
from the primary oracle (V7R1M0, over the LAN, under 1 ms round trip) at
30,700 rows/s [29,700-32,900], prepare to close, after one warm-up run.
That is not like for like with the 288,000 above: the oracle's figure
crosses a real network and reads a catalog view from disk, ours runs on
loopback over a store in memory. No FTP, SFTP or IFS figure was taken
from either oracle.

Limit found on the way: an SQL `CREATE TABLE` took the database's default
member size and filled at 13,000 rows (SQL0901). It now creates the table
with `SIZE(*NOMAX)` as IBM i does; the bench stayed at 10,000 rows.

## ACS checklist (Cody)

Run IBM i Access Client Solutions against a QS/400 image (QEMU with
`hostfwd` to 23, 449 and 8470-8476, or the installed system) and date
each line; this is the item only Cody can sign off.

| # | Check | Expected | Signed off |
|---|---|---|---|
| 1 | Add the system, sign on with a password | the connection test passes for every host server | |
| 2 | Run SQL Scripts: `SELECT * FROM QSYS2.SYSTABLES FETCH FIRST 10 ROWS ONLY` | ten rows | |
| 3 | Integrated File System: browse `/home`, upload and download a file | the file round-trips byte for byte | |
| 4 | Data Transfer to and from a member | the member's records match | |
| 5 | Printer output: open a spooled file | its text shows | |
| 6 | 5250 session over TLS (992) with a certificate assigned by `QSASNCRT` | the session starts; ACS shows the certificate | |
| 7 | `ftp` and `sftp` from the PC to the system | listings, `get` and `put` of a stream file and (FTP) a member | |

## Acceptance checklist

Checked 2026-10-30 on the final tree.

| # | Criterion | Status |
|---|---|---|
| 1 | All tests and goldens green | Pass: `cargo test --workspace` 2,214 passed, 0 failed, 75 ignored (each with its reason) |
| 1 | `cargo run -p build -- test` passes | Pass: 70 of 70 steps, and the 3 media steps (`--media iso`) |
| 1 | JTOpen suite, FTP/SSH/host-server replays, QSH corpus | Pass: the JTOpen suite in QEMU (host-servers commit; step 68 runs it when a JDK and `QS_JT400_JAR` are set), `ftp_replay` (192 sessions), the six host-server replays, `sshd` end to end and against OpenSSH (ignored, run by hand), the `qsh` and `qsh-utils` corpora |
| 1 | ACS checklist signed off | Needs Cody: the seven checks above |
| 2 | HLINV and every Phase 9 corpus program still pass | Pass: `hlinv`, `rpg_kit`, `rpg_fuzz`, `oss_corpus` |
| 2 | Phase 9 open-source projects needing sockets, the IFS or TLS | Partly: HTTPAPI builds and `EXAMPLE1` and `EXAMPLE15` pass (`oss_net`, Linux); `EXAMPLE16`, HTTPAPI over TLS and two si-ibmi-apis modules are `BLOCKED(phase10)` in `PHASE9.md` |
| 3 | `IFS.md`, `TCPIP.md`, `SERVERS.md`, `HOSTSVR.md`, `PRINT.md`, `CERTS.md`, `QSH.md`, this document with the bench | Pass, with the oracle fetch rate above |
| 3 | Spec-versus-code tests: IFS `stat`, sockets structures, host-server code points, exit-point formats | Pass: `sysapi` `ifs`, `sockets` `spec`, `hostserver` `spec`, `regfac` `spec` |
| 4 | `missing_docs = deny` on the new crates; clippy and `cargo deny` clean | Pass: all 25 new crates; clippy (host and musl) and `cargo deny` (295 crates, no C) clean |
| 4 | Shell-utility crates banned, a test over the lockfile | Pass: `deny.toml` bans and `lockfile` |
| 4 | Static binaries; no non-Quartz userland binaries in the image | Pass: the build's ELF check in `build all` |
| 5 | No IBM text outside protocol tokens; tripwire over the new crates | Pass: `phase10_hygiene`; the kept interface strings are listed under Deviations |
| 5 | Oracle material only under `conformance/`; `.env` untracked; no credential tracked | Pass: `phase8_hygiene` |
| 5 | `QSTEST`, `/home/QSTEST`, the pub400 library and home hold nothing this phase made | Pass on the primary oracle (2026-10-30: `QSTEST` empty, `/home/QSTEST` only the 10-22 kit folders). pub400: nothing dated from Phase 10 in the home; the library's `DCL*` and `FEX010` programs are Phase 9's (10-24 before Phase 10 began) and two journal receivers of 10-25 belong to the database kit's journal. Left for Cody to keep or delete |
| 5 | No network configuration changed on either oracle | Pass: only reads, `PING` and the host servers' own sessions |
