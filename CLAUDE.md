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
interim `MAIN` menu, the security APIs and the console's security commands. Phase 7 added
commands: `*CMD` objects and the definition compiler, the one command analyzer
behind every line (`cmd`), the prompter, the 343 shipped definitions and the
inventory of where each runs (`sys-cpps`), the ILE CL compiler to the IR (`cl`),
the service's and the job's processing programs, Command Entry as `QCMD`,
`QCMDEXC`/`QCMDCHK`/`QCAPCMD`, `QSTRUP` in CL, the analyzer and CL kits. Phase 8
added the display interface: a TN5250E server (`QTVTELNET`, virtual devices,
printer sessions, TLS) with the console as its client, display files (`dspf`:
DDS compiler and runtime, `SNDRCVF`, the shipped sign-on), UIM (`uim`: panel
groups, the renderer and dialog engine, the `QUI*` APIs) with the system's
panels, menus (`menus`: `*MENU`, `GO`, generated command menus, task menus),
System Request, Attention and group jobs, the developer tools (SEU, PDM, SDA,
DFU, RLU, STRDBG, STRSQL, STRSST), the oracle driver and the 200-session load
run. Code
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
                per-job contexts answering stg requests, exec of console commands,
                ifs.rs the one Ifs over the xfs and its requests),
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
  diskimg/      GPT layout (ESP, LIC, IFS, ASP1), FAT volumes, partition lookup, the IFS
                partition and its RCLSTG migration (migrate.rs); build + installer
  blockdev/     BlockDev trait, AlignedBuf, Linux O_DIRECT backend, MemBlockDev with faults
  osobjects/    the OS objects of QS00SS1 *BASE (message files, work management objects,
                display files, panel groups and menus, regfac, commands, QSTRUP from
                shipped/QSTRUP.clle): one create() the first IPL and build lp both call
licpgm/       licensed programs (docs/LICPGM.md)
  licpgm/       the model: src/id.rs, src/model/ (definition, load, state), catalog.toml and
                agreements/ (src/catalog.rs), src/payload/ *PRDDFN and *PRDLOD codecs and
                spec layouts, src/directory/ installed status and CHKPRDOPT, src/api/ the
                QSZ*/QLZA* APIs over bytes (dispatch.rs by name), src/plan/ RSTLICPGM,
                DLTLICPGM, SAVLICPGM planning over the Media/Restorer/Remover traits,
                src/pkg/ QSPKGPRD manifests, src/license/ keys, key files, the license
                repository and use, agree.rs software agreements, store.rs ProductStore,
                ObjectCheck, MemStore
  lpstore/      ProductStore and ObjectCheck over the object store (QUSRSYS/QLZALIC,
                QUSRSYS/QSZAGREE), product save streams (SavfMedia, SaveRestorer, save_product)
  lpbuild/      build lp's work: a store in memory, the objects, signed, saved as a product load
  products/     the QSPKGPRD manifests build lp packages (QS00SS1, QS00WDS, QS00TC1, QS00ST1).
                The service side is lic/slic/src/stgsvc/licpgm.rs (seed, the product store,
                the key at /qs400/keys/system.pub), cpp/lpcmd.rs (the commands) and
                cpp/lpapi.rs (panel data, API calls); the job's runtime/sysapi/src/lp.rs and
                work/qsjob/src/panels/licpgm.rs (GO LICPGM, QSZLICPGM)
save/         save and restore (docs/SAVE_FORMAT.md)
  savfmt/       the save stream: media header, records (src/record.rs), object descriptors,
                parts, refs, index and trailer (src/object.rs), chunk compression (codec.rs),
                the content digest (digest.rs), Writer and Reader with read_index and
                descriptor_at; tests/ spec tables and torn, truncated and damaged streams
  saverst/      the engine: image.rs objects as parts (refs zeroed in the stream, typed
                exports), ids.rs references found on save and patched on restore, save.rs
                save_library in IBM's save order (order.rs) under save locks with history,
                restore.rs OPTION/MBROPT/ALWOBJDIF and the rules the oracle settled
                (tools/conformance/oracle/save/primary/README.md), secdta.rs SAVSECDTA,
                RSTUSRPRF and RSTAUT, sign.rs signatures and CHKOBJITG's check, hooks.rs
                the security layer's questions, msg.rs the message IDs. The commands are
                lic/slic/src/cpp/savcmd.rs (save files, SAV*/RST*, DSPSAVF) and savout.rs
                (OUTPUT(*OUTFILE) in QSRSAV and QSRRST)
media/        save media (docs/MEDIA.md)
  tape/         TapeTransport and Changer, VirtualTape and VirtualLibrary, Linux st/sg
                (src/st/, raw.rs the one unsafe file), IBM standard labels (src/label/),
                src/volume/ INZTAP, CHKTAP, DUPTAP, DSPTAP, FileWriter/FileReader across
                volumes, src/device/ TAPnn naming and the *TAPMLB cartridge model
  optical/      ISO 9660 with Joliet and Rock Ridge (src/iso/), the hybrid install image,
                the media manifest, writable virtual volumes (src/volume/), device naming,
                VolumeSet (what /QOPT shows)
  imgclg/       *IMGCLG catalogs over a CatalogFs: model.rs, ops.rs the Registry (every
                command), fs.rs, tape.rs TapeImages
  medio/        a save stream to and from media: tapefile.rs (labeled file, 64 KiB blocks),
                optfile.rs (OPTFILE, saves across optical volumes with .QSPART labels),
                catalog.rs CatalogTapes and NextVolume (the next virtual volume),
                library.rs CatalogLibrary (a virtual *TAPMLB over a *TAP catalog,
                QS400MLB.INF), tapf.rs (*FILE TAPF, the CPYTOTAP/CPYFRMTAP records).
                The service side is lic/slic/src/stgsvc/media/ (devices, catalogs over the
                IFS root, drive, library, stream, operator: QSYSOPR with the replies, PAL,
                problems), cpp/medcmd.rs (the device, catalog, tape, optical and cartridge
                commands, the panels' data) and cpp/tapcpy.rs (CRTTAPF, CHGTAPF, CPYTOTAP,
                CPYFRMTAP); the panels are ui/uim/pnlgrp/QSMEDIA and
                work/qsjob/src/panels/work2/media.rs (WRKIMGCLG, WRKIMGCLGE, WRKTAPCTG,
                WRKMLBSTS)
  instmedia/    install media (docs/INSTALL.md, RECOVERY.md): layout.rs payloads, sign.rs the
                manifest signature, rsp.rs the response file, savsys.rs the layout on tape and
                optical (written, recognised), osload.rs the OS install over a store. The
                installer is lic/slic/src/ipl/install.rs and instdev.rs, SAVSYS cpp/savsys.rs
ptf/          PTFs (docs/PTF.md)
  ptf/          header.rs the PTF header, status.rs the states, package.rs build and open a
                signed PTF stream, db.rs the PTF database, apply.rs LODPTF/APYPTF/RMVPTF
                with QRPLOBJ, group.rs signed PTF groups. The service's commands are
                lic/slic/src/cpp/ptfcmd.rs (the database in QSYS/QSPTFDB, delayed PTFs at IPL)
service/      service: the problem log (docs/SERVICE.md)
  prb/          problem entries and statuses, the log (create, list, delete, report) and
                analyze.rs (QSYSOPR messages, hardware, dumps, ANZPRB). The service side is
                lic/slic/src/stgsvc/prblog.rs and cpp/prbcmd.rs
storage/      the object store and the names and code pages it holds
  store/        object store: src/page/, src/btree/, src/asp/ (units, superblock),
                src/alloc/ (bitmaps, free tree), src/wal/, txn.rs, store.rs, cache.rs, src/obj/
  qsname/       object, library and member name rules; 10-byte CP037 Name
  ccsid/        CP037 <-> UTF-8 from cp037.csv (build.rs), EBCDIC collation; src/conv.rs the
                CCSID conversions (tables from cp/*.csv, UTF-8, UTF-16, 65535)
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
                commands, *JOBQ, *JOBSCD, *OUTQ, *SPLF, *PRTF, *DEVD) and shipped.rs; spec.rs;
                src/opa/ the cleanup and backup options, the backup history, GO SAVE 20 to 23
                (docs/BACKUP.md; the service side lic/slic/src/cpp/opacmd.rs, the job's
                work/qsjob/src/panels/backup.rs)
  qsjob/        the job process: start.rs (control channel, hello, the role); src/console/
                QSVCJOB and every job's runtime (line dispatch, call, debug session, the job
                commands, job log, breaks); src/work/ the roles: monitor, batch (QCMD,
                QWCSTRUP), inter (Command Entry), sysjob, scheduler (QEJSCHED), writer
                (QSPWTR), spawn and cgroup
  spool/        spooling without the store: src/scs/ SCS encode and decode, printer.rs the line
                printer (spacing, skipping, overflow, FCFC), ddsprt.rs DDS printer formats,
                view.rs text, FCFC, *PRTCTL and hex views, pdf.rs the PDF writer
  print/        network printing: src/transform/ the host print transform (ascii, pcl and
                page for the LaserJet models, custom for *WSCST), wscst.rs the customizing
                object, pjl.rs, src/sink/ (raw 9100, ipp, lpr with control.rs, the IBM-to-IBM
                control lines), deliver.rs; tests/ fake servers and PCL goldens
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
command/      commands and CL
  cmd/          command definitions: src/token/ the tokenizer, src/def/ the model,
                src/source/ the definition compiler (rules.toml), src/payload/ the *CMD
                object, src/analyze/ the analyzer (bind, conv, dep, expr), src/block/ the
                parameter block, src/store/ CRTCMD/CHGCMD/CHGCMDDFT/CRTPRXCMD, src/prompt/
                the prompter (layout, session); tests/golden/ the prompt displays
  sys-cpps/     the shipped definitions (cmds/*.cmd), inventory.toml and src/inventory.rs
                (sites, stubs, bridge words, DIRECT/JOB_SITE/CL_STATEMENTS/UNSUPPORTED),
                src/install.rs, src/bridge.rs, denylist.txt; tests/ (defs, the harness
                tripwire, inventory, prompt goldens). The service's processing programs
                are lic/slic/src/cpp/ (direct.rs, clpgm.rs, dbcmd.rs, msgfcmd.rs,
                objcmd.rs, sysinfo.rs), the job's
                work/qsjob/src/command/ (jobsite.rs, natives.rs, prompt.rs)
  cl/           the CL compiler: lex.rs, parse/, sem.rs, lower/ (statements, expr, cmds),
                compile.rs the driver and listing, diag.rs the CPD07xx identifiers
ui/           the display interface
  tn5250/       TN5250E: src/telnet/ negotiation, src/ds/ the 5250 data stream (orders,
                attributes, fields), src/screen/ ScreenModel and its diffing, src/term/ the
                client terminal, record.rs; tests/fuzz.rs
  dspf/         display files: src/compile/ the DDS compiler, src/model/, src/payload/ the
                *FILE descriptors, src/rt/ the runtime (open, write, read, subfiles,
                windows), listing.rs, sys.rs and sys/ the shipped display files
  uim/          UIM: src/lex/, src/tags/, src/compile/ CRTPNLGRP, src/payload/, src/render/
                the renderer rules, src/engine/ the dialog engine behind a Terminal trait,
                shipped.rs; pnlgrp/ the system's panel groups; tests/ the panel goldens
  menus/        menus: src/payload/ *MENU, src/stack/ GO, src/generate/ and bin/genmenus
                (command menus from the inventories), sys/ task menus, pnlgrp/ generated
devtools/     developer tools, pure logic (drivers in work/qsjob/src/devtools/): seu/, pdm/,
              sda/, dfu/ (DFU and RLU), dbg/ (STRDBG screens), isql/ (STRSQL), sst/, edtf/ (EDTF, DSPF)
service/      problems: prb/ the problem log, analysis, the report hook, contact information
              (docs/SERVICE.md); the service side is lic/slic/src/stgsvc/ prblog.rs,
              dumps.rs, trace.rs, liclog.rs, svcop.rs and lic/slic/src/cpp/svccmd.rs
ifs/          the integrated file system
  ifs/          src/errno/ IBM errno values and messages, src/path/ paths and folding,
                src/backend/ (MemBackend, LinuxBackend), src/meta/ the one metadata record
                (codec, RCLLNK), src/auth/ IFS authority and requirements, src/vfs/ mounts,
                devices, /QSYS.LIB, /QOPT; src/resolve/ the resolver and casefold index,
                src/lock/ share modes and byte ranges, src/text/, src/ops/ Ifs and every
                operation, src/save/, src/client/ the IfsClient trait (LocalIfs, Unavailable)
                and its wire codecs; tests/linux-xfs.sh runs the LinuxBackend tests as root
  xfsfmt/       a pure-Rust mkfs for the XFS v5 root (docs/IFS.md "Disk format")
  qsyslib/      the IFS over the store, for the service: view.rs /QSYS.LIB (StoreQsys),
                env.rs profiles, UID/GID and authorization lists, audit.rs QAUDJRN entries,
                savf.rs, access.rs one transaction per call (MemAccess for tests)
  ifscmd/       the IFS commands over an IfsClient: src/cmds/ (dirs, links, auth, attr, udfs,
                admin), display.rs what the displays show, text.rs their text forms,
                src/stmf/ CPYTOSTMF/CPYFRMSTMF and the IMPF stream files, msg.rs IBM's messages
net/          TCP/IP: configuration, sockets, TLS, servers
  servers/      the TCP servers framework (docs/SERVERS.md): src/registry/ every server, its
                jobs, ports and readiness; src/framework/ the TcpServer hooks, the Env, the
                Framework and Starter (small.rs *NTP, *LPD); src/attrs/ CHGTELNA, CHGNTPA,
                CHGLPDA, CHGSMTPA and the HTTP admin ports over the server table's bag;
                listen.rs the accept loop; src/http/ the admin page. The job side is
                work/qsjob/src/command/tcpsvr.rs (netcmd.rs the CHG*A, SNDTCPSPLF/LPR and
                SNDSMTPEMM commands) and the roles work/qsjob/src/work/telnet.rs
                (QTVTELNET), httpadm.rs (ADMIN), ftp.rs (QTFTP00nnn), ntp.rs (QTOTNTP),
                lpd.rs (QTLPD00001)
  ntp/          SNTP: src/packet/ the header and timestamps, client.rs one exchange,
                adjust.rs MINADJ/ADJTHLD/MAXADJ, poll.rs a poll over RMTSYS, server.rs the
                answer and the UDP loop, clock.rs the Clock trait (MemClock for tests)
  lpd/          the line printer daemon: proto.rs RFC 1179 lines, session.rs a connection
                against a Spooler, convert.rs received jobs as spooled files (SCS, FCFC,
                *USERASCII, the QS/400 extension), status.rs the queue state; tests/ LPR
                from work/print over loopback
  mail/         mail out: addr.rs, encode.rs (base64, quoted-printable, RFC 2047, dates),
                mime.rs the SNDSMTPEMM message, smtp.rs the client (STARTTLS, AUTH);
                tests/ a fake smart host
  sshd/         the SSH server (docs/SSHD.md): src/wire/ SSH types, base64, PEM, DER;
                src/transport/ ident, KEXINIT, key exchange (ML-KEM hybrid, X25519,
                P-256), AES-GCM packets; src/keys/ public and private keys,
                authorized_keys, host keys; src/config/ sshd_config and the IBM i paths;
                src/auth/ Accounts, access rules, ssh-userauth; src/conn/ channels,
                windows, forwarding; src/session/ SessionHost, QshHost, the line
                discipline, FileTransfer; src/sftp/ v3 over IfsClient; src/scp/ legacy
                scp; server.rs Server (start, serve), daemon.rs Daemon; src/client/ a
                client for tests; tests/openssh.rs the real clients (ignored)
  nettools/     the network tools: icmp.rs echo packets, cmsg.rs recvmsg control data,
                ping.rs, route.rs (TRACEROUTE), netstat.rs the NETSTAT views, src/cmntrc/ the
                communications trace (trace and byte form, registry, frame.rs, report.rs the
                PRTCMNTRC listing), src/sys/ the Linux sockets (raw.rs the libc calls); the
                service's side lic/slic/src/stgsvc/tcp/netstat.rs, stgsvc/cmntrc.rs,
                cpp/cmntrc.rs; the job's work/qsjob/src/command/nettools.rs and
                panels/tcp/netstat.rs (QTOCNETS)
  ftp/          FTP (docs/FTP.md): src/session/ the server's session (login, dirs, xfer,
                listing, site, info), host.rs the Host trait, names.rs NAMEFMT 0/1,
                objects.rs and store.rs objects in a transfer's form, xlate.rs TYPE A/E/I/C,
                list.rs LISTFMT layouts, src/net/ (tcp, mem) the connections, server.rs,
                block.rs, src/client/ the FTP command's client, memhost.rs a Host in memory;
                tests/ sessions in memory and the client against the server over loopback
docs/         ARCHITECTURE.md, PHASE0.md, PHASE1.md, PHASE2.md, PHASE3.md, STORE_FORMAT.md,
              DST.md, DB_FORMAT.md, DB_OPEN_QUESTIONS.md, DECIMAL.md, QIR.md, ABI.md, ILE.md, MSGF.md, ILE_OPEN_QUESTIONS.md,
              STORAGE_SERVICE.md, WORKMGMT.md, WM_OPEN_QUESTIONS.md, PHASE5.md, SECURITY.md,
              SEC_OPEN_QUESTIONS.md, PHASE6.md, SYSAPI.md, PHASE7.md, CL.md, CMD.md,
              CMD_OPEN_QUESTIONS.md, PHASE8.md, TN5250.md, DSPF.md, UIM.md, MENUS.md,
              TOOLS.md, UI_OPEN_QUESTIONS.md, IFS.md, NET_OPEN_QUESTIONS.md, PRINT.md, SERVERS.md,
              SSHD.md, TCPIP.md, DDM.md, HOSTSVR.md, QSH.md, FTP.md, PHASE10.md, SERVICE.md,
              LICPGM.md, SVC_OPEN_QUESTIONS.md, SAVE_FORMAT.md, MEDIA.md, PTF.md, UPDATE_PROTOCOL.md,
              CONSOLE.md, HARDWARE_NOTES.md, BACKUP.md, images/
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
cargo run -p build -- lp          # signed product loads from licpgm/products/ into target/images/lp/
cargo test -p savfmt -p saverst   # the save stream and the save/restore engine
cargo test -p tape -p optical -p imgclg   # tape, optical, image catalogs
cargo test -p ptf -p prb          # PTFs, the problem log
cargo run -p build -- all         # the whole pipeline (dev: service console on)
cargo run -p build -- all --release       # no qs400.svc=1 on the cmdline
cargo run -p build -- iso         # UEFI-bootable installation media
cargo run -p build -- test        # the QEMU suite: Phases 1 to 11, 78 steps
cargo run -p conformance -- --only 71,75   # some steps only (the boot step always runs)
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
cargo test -p conformance --test cl_kit    # the CL corpus in memory; QS400_UPDATE_CL=1 rewrites expected/
cargo test -p conformance --test oss_net   # HTTPAPI built and its examples run against a loopback HTTP server (Linux); QS400_UPDATE_OSS=1
cargo test -p conformance --test cmd_kit   # the analyzer kit; QS400_UPDATE_CMD=1 rewrites expected.txt
cargo test -p sys-cpps                     # definitions, harness tripwire, inventory, prompt goldens
cargo test --release -p conformance --test cmd_bench -- --ignored --nocapture   # analyzer, prompter, CL benches
cargo run -p conformance -- --load 200    # 200 TN5250 sessions under QEMU, timings to PHASE8.md
cargo run -p tn5250-driver -- run --target pub400 [--golden DIR --update] SCRIPT.tn   # oracle captures
cargo run -p menus --bin genmenus         # regenerate the command menus in ui/menus/pnlgrp/
cargo test -p uim -p dspf -p menus        # panel, display-file and menu goldens
cargo test -p seu -p pdm -p sda -p dfu -p dbg -p isql -p sst -p edtf   # the developer tools' screens
cargo test -p ifs                         # the IFS over MemBackend
cargo test -p nettools                    # PING, TRACEROUTE, NETSTAT rows, the communications trace
cargo test -p ifscmd                      # the IFS commands and CPYTOSTMF/CPYFRMSTMF over MemBackend
cargo test -p qsyslib -p stg              # /QSYS.LIB over a store; the IFS over the link
cargo test -p print                       # the host print transform, PCL goldens, fake printer servers
cargo test -p ftp                         # FTP sessions in memory, the client against the server over loopback
cargo test -p tape -p optical -p imgclg -p medio   # tape, optical, image catalogs, saves on media
cargo test --release -p saverst --test media_bench -- --ignored --nocapture   # save/restore MiB/s to virtual tape and optical
cargo test -p conformance --test ftp_replay   # the 192 recorded pub400 FTP sessions; report in target/conformance-ftp/
cargo test -p servers                     # the servers framework, CHGTELNA, the HTTP admin listener over loopback
cargo test -p ntp -p lpd -p mail          # SNTP, the LPD with LPR over loopback, MIME and SMTP against a fake host
cargo test -p sys-cpps --test smallsvr    # CHGNTPA, CHGLPDA, CHGSMTPA, SNDTCPSPLF, LPR against the oracle
cargo test -p sshd                        # the SSH server end to end in memory
cargo test -p licpgm -p lpstore -p lpbuild -p osobjects   # products, payloads, APIs, keys; the store; build lp
cargo test -p uim --test licpgm_panels    # GO LICPGM panels; QS400_UPDATE_GOLDEN=1 rewrites tests/golden/licpgm/
cargo test -p sshd --test openssh -- --ignored   # real ssh/sftp/scp against the daemon
wsl -d Ubuntu -u root -- bash ifs/ifs/tests/linux-xfs.sh   # LinuxBackend over an xfs loop image
```

Lint the Linux-only crates for the musl target too: their real code is
`cfg(target_os = "linux")` and the host build only sees stubs.

A line at the service console is a CL command string run through the
analyzer (`crtlib LIB(X) TEXT('made')`, `?crtpf`); the dev words below are
reached with the `svc.` prefix only (`svc.obj ls QGPL`, `svc.xfer put`).
Service console (serial, dev builds): `help`, `srcs [table]`, `hist`, `hw`,
`lic`, `sysval NAME [VALUE]`, `mode manual|normal`, `screen`, `pwrdwn
[restart]`, `release`; store: `asp`, `lib ls`, `obj ls|crt|dlt|rnm|mov|dsp|bench`,
`dtaara get|set`, `dtaq snd|rcv`, `usrspc rd|wr`, `rclstg [*CHECK]`, `chkstg`,
`txn stats`, `ifs` (the IFS mount at /run/qs400/ifs, capacity, free space),
`sysdump [take|list|show N|delete N]` (system dumps), `liclog [MAJOR]` (the LIC log),
`unit add|rmv`, `mirror start|stop|suspend|resume|replace|status`,
`key NAME`, `type TEXT` (`QTEMP/NAME` addresses the temporary ASP); database:
`xfer put|get|ls|rm` (named line buffers pushed over serial), `crtpf`, `crtlf`,
`dltf`, `addpfm`, `rmvm`, `clrpfm`, `rgzpfm`, `cpyf`, `cpytoimpf`, `cpyfrmimpf`,
`inzpfm`, `chgpf` (SRCFILE), `dspfd`, `dspffd`, `dsppfm`, `dspdbr`, `srcput`, `srcget`, `crtjrnrcv`, `crtjrn`,
`chgjrn`, `dltjrn`, `dltjrnrcv`, `dspjrnrcva`, `strjrnpf`, `endjrnpf`, `dspjrn`,
`apyjrnchg`, `rmvjrnchg`, `strcmtctl`, `commit`, `rollbck`, `endcmtctl`, `ovrdbf`,
`dltovr`, `ovrmsgf`, `opnqryf`, `clof`, `sql` (line mode, `;` ends a statement, `quit` leaves), `sqlrun NAME`
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
`rlsjobq`, `clrjobq`, `hldjob`, `rlsjob`, `endjob`, `dlyjob`, `chgjobq`, `addjobscde`,
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
`prtjobdaut`, `prtsbsdaut`, `prtqaut`, `prtusrprf`, `anzprfact`; IFS: `ifs` (the mount),
`ifs ls [PATH]|cat PATH|put PATH TEXT|mkdir PATH|rm PATH|stat PATH|df [PATH]` (through the
service's Ifs as the line's job); and `dbgkill JOB()` (slic's debug hook:
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
  `runtime/mi/src/rtlib/mem.rs` (the storage and job behind a runtime call),
  `runtime/mi/src/act.rs` (calling into generated code: entries and handlers),
  `work/qsjob/src/platform/stack.rs` (the stack guard's signal handler),
  `net/nettools/src/sys/raw.rs` (`setsockopt`, `recvmsg` with control data,
  `AF_PACKET`), `media/tape/src/st/raw.rs` (the `st` and `sg` ioctls) and
  `work/qsjob/src/platform/clock.rs` (`adjtimex`, which musl's libc binding lacks).
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
- Oracle credentials live only in `tools/conformance/oracle/.env`
  (gitignored); tools read them from the environment, probe scripts
  write the account as `{user}`, and `phase8_hygiene` fails on any
  value of it (in any case) in the tree.
- Framebuffer writes: one `pwrite` per frame spanning every line, then
  `fsync`. Partial-line writes lose damage at line ends and `mmap` never
  flushes under simpledrm's fbdev emulation (see docs/PHASE1.md).
