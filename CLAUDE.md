# QS/400 (Quartz Systems/400)

A from-scratch operating system on the Linux kernel, written entirely in Rust,
source-compatible with IBM i and screen-identical at a 5250 terminal.
Phase 0 built the build system and a booting kernel with a Rust PID 1
(`slic`). Phase 1 made `slic` a real IPL with persistent state, system
values and a local 5250-style console. Phase 2 added the object store
(`store`), the disk layout with an ASP partition, system values in the
`QSYSVAL` object, RCLSTG, the service console's object commands and the
ISO installer. Addendum 2.5 added mirrored protection, Dedicated Service
Tools (`svctools`) and the IBM-style install flow. Phase 3 added the
database: DDS, decimal arithmetic, files and access paths, record-level
access, journaling, commitment control, SQL and the QSTEST conformance kit. Phase 4 added
the ILE runtime: the IR (`qir`), the Cranelift code generator (`codegen`), ILE objects
and the binder (`ile`), the runtime with its job stand-in, exception model, heaps, user
spaces, messages, file I/O and debugger (`mi`), message files (`msgf`), the system APIs
(`sysapi`), the service console's ILE commands and the IR conformance kit. Phase 5 added
work management: the storage service (`/init qsstg`) and one `qsjob` process per job,
job descriptions, classes, library lists, `*LDA`, job logs, message queues, `QSYSOPR`,
`QHST`, the reply list, subsystems with pools as cgroups, routing, the console's
sign-on and Command Entry, job queues, `SBMJOB`, the job scheduler, output queues,
spooled files, printer files (SCS, DDS), writers with the capture device and PDF, the
system jobs, `QSTRUP`, `PWRDWNSYS`, and the statistics displays. Phase 6 added
security: user profiles and passwords (`sec`), the authority model with private
authorities, authorization lists and IBM's search, enforced by a guard inside the
store, adopted authority, `QSECURITY` 40 and 50 domain and state checks, the audit
journal `QAUDJRN`, the security system values, a real console sign-on with the
interim `MAIN` menu, the security APIs and the console's security commands. Code
waiting for a later phase is marked `MIGRATE(phaseN)` or `TODO(phaseN)`.

## Ground rules

- Every source file starts with the license header (see `LICENSE`):
  ```
  // Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
  // licensed under the terms of the MIT license which can be found in the
  // root directory of this project.
  ```
- Comments are short and to the point.
- Claude commits at each natural checkpoint, as Cody (the repo's git user),
  with a one-line message under 60 chars and no Claude attribution or
  `Co-Authored-By` lines, then carries on.
- Pure Rust only. No crate may compile or link C (`cc`, `cmake`, `bindgen`,
  `links =`). `deny.toml` enforces this; `cargo run -p build -- deny` checks it.
- Source files are grouped into folders by concern (`host/`, `kernel/`,
  `image/`, ...), not left as one flat list under `src/`. Crates likewise
  live in a category folder by OS layer (`tools/`, `lic/`, `storage/`,
  `database/`, `runtime/`, `work/`), never at the repository root.

## Locked decisions

See `docs/ARCHITECTURE.md`. Short form:

- x86_64 is the only supported architecture. There is no second target and
  no arch abstraction in the code. The portable IR of a later phase exists
  for codegen upgrades, not for architecture migration.
- Linux kernel is the only inherited component. Userland is 100% Rust, static
  musl (`x86_64-unknown-linux-musl`).
- Kernel 6.18.y, LTS, built with `LLVM=1`, monolithic (`MODULES=n`), EFI stub,
  initramfs embedded. The bzImage is the UKI. Two ESP slots, A and B.
- Pin lives in `kernel/VERSION` and `kernel/SHA256`. Bumps are their own commit.
- Release identity is `V1R0M0` from the `release` crate; see ARCHITECTURE.md.
- Command line is baked: `console=ttyS0,115200 console=tty0 panic=-1 quiet`.
- Reproducible: fixed `SOURCE_DATE_EPOCH`/`KBUILD_BUILD_*`, deterministic cpio,
  deterministic GPT/FAT, signed content-hash manifest.

## Layout

```
kernel/       VERSION, SHA256, config/qs400.config
tools/        host-side tools
  build/        orchestrator: cargo run -p build -- <cmd>
    src/host/     cmd, paths, hash, doctor, deny, testrun
    src/kernel/   fetch, config, build (slot A and B UKIs)
    src/userland/ slic build + ELF check
    src/image/    cpio, initramfs, disk (via diskimg), iso (ISO 9660), media, manifest
  conformance/  QEMU harness: src/qemu/ (session, qmp, serial, firmware), src/steps.rs;
                src/db/ the database kit (kit, local run, compare, oracle RPG, QEMU steps);
                db/ the QSTEST kit: dds/, data/, rla/, sql/, oracle/, expected/
                src/qir/ the IR kit runner (cases, the in-memory job with CUST, *LDA,
                COUNTER, Q1 and the APIs); qir/ the kit: programs/*.qir, expected/*.out
lic/          Licensed Internal Code: IPL, platform, console, DST, disks
  slic/         PID 1 and, as `/init qsstg`, the storage service: src/ipl/ state machine,
                src/hwrsrc/, src/net/, src/procs/ (spawn, the storage service handover and
                close, QSVCJOB line routing), src/stgsvc/ (the service: open, serve loop,
                per-job contexts answering stg requests, exec of console commands),
                src/svc/ (db/ holds the database, journal, SQL, rla and dbbench console
                commands; ile/ the ILE build and display commands and the job sources over
                the lent store with the sysapi provider),
                src/platform/ (console, mounts, power, rtc, serial, sys), src/src.rs
  licpart/      LIC partition: A/B record (src/format/), locate by GPT type GUID
  sysval/       catalog.toml, validation, src/tz/ time zones, LIC storage
  release/      VxRyMz identity, build ID, PTF names (one source of truth)
  console/      font/qs400-9x16.txt, src/fb/ framebuffer, src/keyboard/, screens
  svctools/     DST screens and the installer's: src/ui/ screen kit, panel.rs runner,
                src/dst/ (disk/, environment, tools, security), src/install/, src/users/
  diskimg/      GPT layout (ESP, LIC, ASP1), FAT volumes, partition lookup; build + installer
  blockdev/     BlockDev trait, AlignedBuf, Linux O_DIRECT backend, MemBlockDev with faults
storage/      the object store and the names and code pages it holds
  store/        object store: src/page/, src/btree/, src/asp/ (units, superblock),
                src/alloc/ (bitmaps, free tree), src/wal/, txn.rs, store.rs, cache.rs, src/obj/
  qsname/       object, library and member name rules; 10-byte CP037 Name
  ccsid/        CP037 <-> UTF-8 from cp037.csv (build.rs), EBCDIC collation
database/     DDS, decimal, files, journals, SQL
  db/           database: src/file/ *FILE attributes, member and dependents tables, LF
                creation; src/format/ sections and level IDs; src/record/ field values;
                src/path/ key encoding and path trees; src/lf/ derivation and select/omit;
                src/member/ *MBR objects, record slots, path maintenance and rebuild;
                src/odp/ open data paths, RLA operations, record locks, INFDS feedback,
                OVRDBF overrides and the per-job session with shared paths;
                src/util/ CLRPFM, RGZPFM, CPYF (select, map), CPYTOIMPF/CPYFRMIMPF (impf);
                src/dsp/ DSPFD, DSPFFD, DSPPFM, DSPDBR; src/chg/ CHGPF, CHGLF, CHGPF SRCFILE;
                src/source/ source members (srcput, srcget, DDS source of a member);
                src/journal/ STRJRNPF/ENDJRNPF, the entries members and paths write,
                APYJRNCHG/RMVJRNCHG (undo.rs) and IPL recovery (recover.rs); src/cmt/
                commitment control (STRCMTCTL, COMMIT, ROLLBCK, lock levels)
  dds/          DDS: src/source/ records, src/lex/ columns and keywords, src/compile/
                field, key, select/omit and file-keyword builders, src/pf/ and src/lf/
                compilers, src/listing/
  jrn/          journaling: *JRN (src/journal/) and *JRNRCV (src/rcv/) objects, the entry
                layout (src/entry/), DSPJRN filters and the receiver chain cursor (src/dsp/)
  sql/          SQL: src/lex/ tokens, src/ast/ statements (ddl, query, expr), src/parse/
                recursive descent (ddl, dml, query, expr); src/value/ runtime values;
                src/exec/ context and name resolution (mod.rs), expressions (eval.rs),
                scalar functions (func.rs), queries with path choice (query.rs), INSERT/
                UPDATE/DELETE/MERGE through the ODP (dml.rs), CREATE TABLE/INDEX/VIEW and
                labels as DDS (ddl.rs), cursors (cursor.rs), QSYS2 catalog views computed
                from the store (catalog.rs); src/sqlca.rs the SQLCA; src/run.rs RUNSQLSTM
  qdecimal/     Decimal (63 digits), IBM intermediate precision rules (src/arith/),
                packed, zoned, binary, integer and float encodings (src/encode/),
                edit codes and words (src/edit/), the IBM i oracle kit (src/fuzz/,
                src/bin/qdfuzz.rs; files under conformance/oracle/qdecimal/)
runtime/      ILE: IR, code generator, runtime, objects, APIs, messages
  qir/          the IR: src/types/ layouts, src/module/ units, vars, procs, src/inst/ the
                instruction set and literal encoding, src/text/ lexer, parser, printer,
                src/verify/ places, typing, flow, monitors, per-instruction checks,
                src/binary/ the versioned module format, src/bin/qirc.rs; tests/programs/
  codegen/      IR to x86_64 through Cranelift: src/abi.rs env record, signatures, static
                frames; src/rt.rs the runtime call table; src/lower/ driver, places, scalar
                and control-flow instructions, calls; src/output.rs code, relocations, maps;
                src/load.rs placing code and resolving relocations; tests/common/ a stand-in
                runtime
  mi/           the runtime: src/job.rs the job stand-in, src/act.rs loaded programs,
                activation groups and the call stack, src/space.rs fat pointers and spaces,
                src/heap.rs heaps, src/usrspc.rs mapped user spaces, src/msg.rs message
                queues, src/cond.rs conditions and RPG status codes, src/except.rs the
                handler search, function check, inquiry and cancel, src/io/ the IoSource
                and DataSource traits, MemData and the store-backed StoreIo, src/debug.rs STRDBG/
                breakpoints/steps/EVAL over the debug-translated variant, src/cee.rs CEEHDLR/
                CEERTX/CEESGL/CEENCOD/CEEDCOD/CEEMRCR as bound procedures, src/dump.rs job
                dumps, src/rtlib/ the call table's functions (core, decimal, string,
                convert, date, heap, msg, io, debug; mem.rs holds the unsafe views),
                src/exec.rs running one module; tests/ programs, activation, heap, messages,
                except, io, debug
  ile/          ILE objects: src/object/ *MODULE, *PGM/*SRVPGM and *BNDDIR payloads and the
                native-output codec; src/binder/ the binder, binder language, signatures;
                src/dsp/ DSPPGM, DSPSRVPGM, DSPMOD, DSPPGMREF, DSPBNDDIR text; src/store/
                CRTMOD/CRTPGM/CRTSRVPGM/UPDPGM/CRTBNDDIR against the store; src/spec.rs
  sysapi/       system APIs as natives of QSYS and QLEAWI procedures: src/param.rs parameter
                and receiver conventions, ERRC0100/ERRC0200; src/provider.rs the Provider trait
                (objects, data queues and areas, user indexes, files, system values, commands,
                the clock) and MemProvider; src/msg.rs QMH*, src/usrspc.rs QUS user spaces and
                indexes, src/obj.rs QUSLOBJ/QUSROBJD/QLICOBJD/QLIRNMO and the QUSGEN list
                header, src/data.rs data queues and areas, src/job.rs QUSRJOBI/QUSLJOB/
                QWCRSVAL/QWCCVTDT/QWCRTVCA/QWCRTVTZ/QSZRTVPR/QLGCNVCS/QWCRNETA, src/db.rs
                QUSLMBR/QUSRMBRD/QUSLFLD/QUSLRCD/QDBRTVFD/QDBLDBR, src/misc.rs commands,
                security, commitment and spool stubs, src/cee.rs the rest of QLEAWI,
                src/picture.rs date pictures and the Lilian calendar; tests/apis.rs
  msgf/         message files: src/desc.rs descriptions and substitution, src/source.rs the
                TOML sources (msgs/*.toml embedded), src/payload.rs the *MSGF payload,
                src/store.rs CRTMSGF/ADDMSGD/CHGMSGD/RMVMSGD/MRGMSGF/RTVMSG/DSPMSGD and the
                first-IPL install; msgs/ qcpfmsg.toml, qceemsg.toml, qsqlmsg.toml
work/         work management: jobs, spooling, process messages
  wm/           work management: cmd.rs console command parameters, ctl.rs the job control
                channel (Start, ToJob, FromJob); src/job/ ids, types, statuses, attributes and
                the library list; joblog.rs; table.rs the active job table; pools.rs activity
                levels; stats.rs the /proc, cgroup and block parsers; wtr.rs; src/obj/ the
                objects' payloads (*JOBD, *CLS, *MSGQ, the reply list, *SBSD and its
                commands, *JOBQ, *JOBSCD, *OUTQ, *SPLF, *PRTF, *DEVD) and shipped.rs; spec.rs
  qsjob/        the job process: start.rs (control channel, hello, the role); src/console/
                QSVCJOB and every job's runtime (line dispatch, call, debug session, the job
                commands, job log, breaks); src/work/ the roles: monitor, batch (QCMD,
                QWCSTRUP), inter (Command Entry), sysjob, scheduler (QEJSCHED), writer
                (QSPWTR), spawn and cgroup
  spool/        spooling without the store: src/scs/ SCS encode and decode, printer.rs the line
                printer (spacing, skipping, overflow, FCFC), ddsprt.rs DDS printer formats,
                view.rs text, FCFC, *PRTCTL and hex views, pdf.rs the PDF writer
  ipc/          process messages: src/wire/ the Wire codec and wire_struct!/wire_enum!,
                frame.rs length-prefixed frames, rpc.rs the rpc! macro, chan.rs Unix sockets
  stg/          storage service protocol: proto.rs the requests (rpc!), link.rs a job's link,
                src/remote/ the runtime's source traits answered by the service
security/     users, authorities, auditing
  sec/          security: src/profile/ the *USRPRF payload, special authorities, the profile
                objects and indexes, the shipped profiles, groups and limits; src/password/
                Argon2id hashes, the QPWD* rules and the policy; src/auth/ bits, grants,
                authorization lists, the principal, the search, the cache and required.rs
                (required.toml); guard.rs the store's guard; src/audit/ kinds, layouts, the
                writer and QAUDENDACN; sysval.rs; level.rs; signon.rs; api.rs the APIs'
                questions; src/cmd/ the commands as library functions (usrprf, object,
                dltusrprf, audit); required.toml, exempt.toml; tests/
docs/         ARCHITECTURE.md, PHASE0.md, PHASE1.md, PHASE2.md, PHASE3.md, STORE_FORMAT.md,
              DST.md, DB_FORMAT.md, DB_OPEN_QUESTIONS.md, DECIMAL.md, QIR.md, ABI.md, ILE.md, MSGF.md, ILE_OPEN_QUESTIONS.md,
              STORAGE_SERVICE.md, WORKMGMT.md, WM_OPEN_QUESTIONS.md, PHASE5.md, SECURITY.md,
              SEC_OPEN_QUESTIONS.md, PHASE6.md, SYSAPI.md, images/
```

## Commands

Run from Linux (WSL Ubuntu on this machine). See `docs/PHASE0.md` for host setup.

```
cargo run -p build -- doctor      # host prerequisites table
cargo run -p build -- fetch       # kernel tarball, sha256, extract
cargo run -p build -- kernel
cargo run -p build -- userland
cargo run -p build -- initramfs
cargo run -p build -- uki
cargo run -p build -- disk
cargo run -p build -- manifest
cargo run -p build -- all         # the whole pipeline (dev: service console on)
cargo run -p build -- all --release       # no qs400.svc=1 on the cmdline
cargo run -p build -- iso         # UEFI-bootable installation media
cargo run -p build -- test        # the QEMU suite: Phases 1 to 6, 45 steps
cargo run -p build -- test --media iso    # the media steps
cargo run -p build -- test --media iso --install   # automatic install, boot, interactive reinstall
cargo run -p build -- deny        # cargo-deny + links/cc lockfile check
cargo clippy --workspace --all-targets    # must be warning-free
cargo clippy -p slic -p console --all-targets --target x86_64-unknown-linux-musl
QS400_QEMU=1 cargo test -p conformance    # the suite as a cargo test
cargo run -p qdecimal --bin qdfuzz -- kit tools/conformance/oracle/qdecimal   # regenerate the oracle kit
cargo test -p conformance --test db_kit   # QSTEST in memory; outputs under target/conformance-db/ours/
QS400_UPDATE_ORACLE=1 cargo test -p conformance --test db_kit   # regenerate RLATEST.RPGLE and QRESULTS.SQL
cargo test --release -p conformance --test db_bench -- --ignored --nocapture   # dbbench in memory
cargo run -p qir --bin qirc -- asm|dis|check|fmt FILE   # IR assembler, disassembler, verifier
cargo test -p conformance --test qir_kit   # the IR kit in memory; outputs under target/conformance-qir/ours/
QS400_UPDATE_QIR=1 cargo test -p conformance --test qir_kit   # rewrite conformance/qir/expected/
cargo test --release -p conformance --test callbench -- --ignored --nocapture   # a million calls, timed
```

Lint the Linux-only crates for the musl target too: their real code is
`cfg(target_os = "linux")` and the host build only sees stubs.

Service console (serial, dev builds): `help`, `srcs [table]`, `hist`, `hw`,
`lic`, `sysval NAME [VALUE]`, `mode manual|normal`, `screen`, `pwrdwn
[restart]`, `release`; store: `asp`, `lib ls`, `obj ls|crt|dlt|rnm|mov|dsp|bench`,
`dtaara get|set`, `dtaq snd|rcv`, `usrspc rd|wr`, `rclstg [*CHECK]`, `chkstg`,
`txn stats`, `unit add|rmv`, `mirror start|stop|suspend|resume|replace|status`,
`key NAME`, `type TEXT` (`QTEMP/NAME` addresses the temporary ASP); database:
`xfer put|get|ls|rm` (named line buffers pushed over serial), `crtpf`, `crtlf`,
`dltf`, `addpfm`, `rmvm`, `clrpfm`, `rgzpfm`, `cpyf`, `cpytoimpf`, `cpyfrmimpf`,
`dspfd`, `dspffd`, `dsppfm`, `dspdbr`, `srcput`, `srcget`, `crtjrnrcv`, `crtjrn`,
`chgjrn`, `dltjrn`, `dltjrnrcv`, `dspjrnrcva`, `strjrnpf`, `endjrnpf`, `dspjrn`,
`apyjrnchg`, `rmvjrnchg`, `strcmtctl`, `commit`, `rollbck`, `endcmtctl`, `ovrdbf`,
`dltovr`, `sql` (line mode, `;` ends a statement, `quit` leaves), `sqlrun NAME`
(RUNSQLSTM over an xfer buffer), `naming`, `curlib`, `rla [NAME]` (scripted RLA
with expected status codes), `dbbench N`; ILE: `qir asm|check|dis`, `crtpgm`,
`crtsrvpgm`, `updpgm`, `crtbnddir`, `addbnddire`, `call LIB/PGM [parms]`, `dsppgm`,
`dspsrvpgm`, `dspmod`, `dsppgmref`, `dspbnddir`, `dspactgrp`, `rclactgrp`,
`dspcallstk`, `dspmsgq`, `rpy`, `dmp`, `wrkmsgd`, `dspmsgd`, `strdbg`, `addbkp`,
`rmvbkp`, `dspbkp`, `enddbg` (at a stop: `step`, `go`, `eval`, `dspvar`); work
management: `wrkactjob [VIEW(1|2|3)]`, `wrkjob`, `dspjob`, `chgjob`, `dspjoblog`,
`crtjobd`/`chgjobd`/`dspjobd`, `crtcls`/`dspcls`, `dsplibl`, `chglibl`, `addlible`,
`rmvlible`, `chgcurlib`, `sndmsg`, `sndbrkmsg`, `sndrpymsg`, `dspmsg`, `rcvmsg`,
`crtmsgq`, `chgmsgq`, `dsplog`, `wrkrpyle`, `addrpyle`, `rmvrpyle`, `crtsbsd`,
`dspsbsd`, `addrtge`, `addaje`, `addjobqe`, `chgjobqe`, `strsbs`, `endsbs`, `wrksbs`,
`wrksbsjob`, `wrkshrpool`, `chgshrpool`, `sbmjob`, `wrkjobq`, `crtjobq`, `hldjobq`,
`rlsjobq`, `clrjobq`, `hldjob`, `rlsjob`, `endjob`, `dlyjob`, `addjobscde`,
`chgjobscde`, `rmvjobscde`, `hldjobscde`, `rlsjobscde`, `wrkjobscde`, `crtoutq`,
`chgoutq`, `dltoutq`, `hldoutq`, `rlsoutq`, `clroutq`, `wrkoutq`, `wrkoutqd`,
`wrksplf`, `wrksplfa`, `dspsplf`, `cpysplf`, `chgsplfa`, `hldsplf`, `rlssplf`,
`dltsplf`, `crtprtf`, `chgprtf`, `ovrprtf`, `strprtwtr`, `endwtr`, `hldwtr`,
`rlswtr`, `chgwtr`, `wrkwtr`, `prtcapture list|get|clear`, `wrksyssts`, `wrkdsksts`,
`wrksysact`, `wmstats`, `pwrdwnsys`, `endsys`; security: `whoami`, `crtusrprf`,
`chgusrprf`, `dltusrprf`, `dspusrprf`, `wrkusrprf`, `dspautusr`, `anzdftpwd`,
`grtobjaut`, `rvkobjaut`, `dspobjaut`, `edtobjaut`, `chgobjown`, `chgobjpgp`, `chkobj`,
`grtusraut`, `wrkobjown`, `crtautl`, `addautle`, `chgautle`, `rmvautle`, `dspautl`,
`dspautlobj`, `chgsecaud`, `dspsecaud`, `chgobjaud`, `chgusraud`, `dspaudjrne`,
`cpyaudjrne`, `prtadpobj`, `prtusrobj`, `prtpvtaut`, `prtpubaut`, `prtsysseca`,
`prtjobdaut`, `prtsbsdaut`, `prtqaut`, and `dbgkill JOB()` (slic's debug hook:
SIGKILL a job). Parameters are IBM-style `KEYWORD(value)`. Boot parameters: `qs400.slot=A|B`, `qs400.mode=normal|manual`,
`qs400.svc=1`, `qs400.ipl=D`, `qs400.media=iso`, `qs400.pace=MS` (each IPL code and
install step stays up this long; 400 by default, 0 for none).

`QS400_TARGET_DIR` relocates `target/` (use an ext4 path when the repo is on
`/mnt/c`; drvfs is far too slow for a kernel build). `-v` streams tool output;
otherwise logs go to `target/logs/`.

## Conventions

- Edition 2024, toolchain pinned in `rust-toolchain.toml`.
- Workspace lints: `unsafe_op_in_unsafe_fn = deny`, clippy `pedantic` warn,
  `missing_docs` warn. Clippy must stay clean.
- Every `unsafe` block has a `// SAFETY:` comment. `unsafe` lives only in
  `lic/slic/src/platform/sys.rs` (signals), `lic/slic/src/platform/rtc.rs` (one
  ioctl), `lic/console/src/fb/raw.rs` (framebuffer ioctls),
  `lic/slic/src/platform/blk.rs` (BLKRRPART), `lic/blockdev/src/aligned.rs`
  (aligned buffer views), `lic/blockdev/src/linux.rs` (BLKDISCARD),
  `runtime/mi/src/rtlib/mem.rs` (the storage and job behind a runtime call) and
  `runtime/mi/src/act.rs` (calling into generated code: entries and handlers).
- Errors: `thiserror` enums in libraries, `anyhow` only in binaries.
- Logs: `tracing`.
- Small documented modules, one responsibility each. Private modules in
  binaries, `pub` only where a sibling needs it.
- Downloaded sources live under `target/src/` and are untrusted. Nothing is
  executed from there except the kernel's own `make` and `merge_config.sh`,
  always invoked with explicit paths.
- SRC codes live in one table, `lic/slic/src/src.rs`; see docs/PHASE1.md.
- Every string a user sees is Quartz-written; never reproduce IBM message
  or help text. IBM names (system values, SRC families, screen geometry,
  DST menu titles and option numbers) are the compatibility interface.
- Every DST and installer screen has a golden file under
  `tools/conformance/golden/`; `QS400_UPDATE_GOLDEN=1 cargo test -p svctools`
  rewrites them after a deliberate change.
- IPL state (slots, history, keylock, the IPL subset of the system
  values) stays in `licpart`; everything else persists in the object
  store.
- On-disk store structures are specified in `docs/STORE_FORMAT.md` before
  they are coded; `storage/store/tests/spec.rs` compares the code's `Layout`s with
  the spec tables, so both change together. `docs/DB_FORMAT.md` and
  `database/db/tests/spec.rs` do the same for the database layouts.
- Framebuffer writes: one `pwrite` per frame spanning every line, then
  `fsync`. Partial-line writes lose damage at line ends and `mmap` never
  flushes under simpledrm's fbdev emulation (see docs/PHASE1.md).
