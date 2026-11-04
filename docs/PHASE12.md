# Phase 12: release engineering (V1R1M0)

Phase 12 takes QS/400 from "installs and runs" to releasable: it closes
the gaps earlier phases carried, freezes the on-disk and on-wire
formats, adds the operational subsystems a release needs and cuts
V1R1M0. This file is the inventory, the carried items, the deviations
and the acceptance state. It grows with the phase.

| # | Section | Document |
|---|---|---|
| 0 | Housekeeping: this file, `CLAUDE.md`, triage | this file |
| 1 | Close the carried gaps | this file, § Carried items |
| 2 | Format freeze and V1R1M0 | `STORE_FORMAT.md`, `SAVE_FORMAT.md`, `PTF.md`, `UPDATE_PROTOCOL.md`, `QAP.md` |
| 3 | Collection Services and performance tools | |
| 4 | Help everywhere | |
| 5 | Text audit and national language support | |
| 6 | Hardware qualification | `QUALIFIED_HARDWARE.md` |
| 7 | Management API and the QS-VS guest agent | `QAP.md` |
| 8 | RPG III and `CVTRPGSRC` | `RPG.md` |
| 9 | Hardening | |
| 10 | Knowledge Center and exit review | `RELEASE_NOTES_V1R1M0.md` |
| 11 | Cut the release | |

## Notes on the brief

The brief names some places by other paths than the tree has. Where it
says `conformance/` the tree has `tools/conformance/`;
`command/ddm.rs` is `work/qsjob/src/command/ddm.rs`;
`work/hostsvr/job.rs` is `work/qsjob/src/work/hostsvr/job.rs`;
`work/telnet.rs` is `work/qsjob/src/work/telnet.rs`; the `ebcdic`
crate is `storage/ccsid`; the oracle `.env` is
`tools/conformance/oracle/.env`. The security model the brief places in
Phase 8 is Phase 6's (`SECURITY.md`).

Three requests conflict with standing rules or with the clock, and are
handled as follows (each is also a row in the acceptance table):

- **`cargo fuzz` is libFuzzer**, a C++ library that `libfuzzer-sys`
  compiles with `cc`. That breaks the pure-Rust rule `deny.toml`
  enforces. The fuzz targets run under a fuzzer written in Rust in the
  tree instead (`tools/fuzz`, `build fuzz`), with the same target shape
  (`fn(&[u8])`), saved corpora and crash minimisation.
- **Long runs are short.** At Cody's request (2026-11-03) nothing in
  Phase 12 runs for hours: each fuzz target runs 3 minutes, six at a
  time (about 10 minutes for all 18), the soak runs one hour, and QEMU
  runs only the steps a change touches.
- **The SMTP server.** QS/400 has an SMTP client (`net/mail`, with
  `STARTTLS`, `AUTH PLAIN` and `AUTH LOGIN` already) and no SMTP
  server. Section 1 closes the client's credential storage; a server is
  out of scope for V1R1M0 and listed in the release notes.

## Pre-existing out-of-order work

The web licensed programs in `net/` (`net/qsweb`, `net/httpd`,
`net/httpadmin`, `net/navigator`) and the QAP server `net/qapd` were
started before Phase 12, against formats that were not yet frozen.
They are post-Phase-12 licensed programs: this phase touches them only
where Section 2 (the format freeze) and Section 7 (the management API
over QAP) require, and each must be re-verified in its own handoff once
Section 2 has landed.

## Carried items

One row per `BLOCKED`, `MIGRATE` and `TODO` marker, per "not available
in this release" site, per test ignored for want of a golden and per
open question, as found at `93c6350f`. "Closes" names the Phase 12 item
that removes it; "Stays" says why it remains. The State column is
updated as rows close.

### Markers in code

| Where | What | Closes / stays | State |
|---|---|---|---|
| `rpg/compiler/src/files.rs:499` | `LIKEFILE` parameters (a file passed to a procedure) | §1 RPG: `LIKEFILE` | open |
| `rpg/compiler/src/codegen/xml.rs:303` | `DATA-INTO`/`DATA-GEN` call no parser or generator | §1 RPG: `DATA-INTO`/`DATA-GEN` | open |
| `rpg/compiler/src/codegen/dbio.rs:153` | Open Access handlers are not called | §1 RPG: Open Access | open |
| `rpg/qrpgrt/src/ctl.rs:41` | `DUMP(A)` writes no formatted dump | §1 RPG: `DUMP(A)` | open |
| `command/cl/src/lower/expr.rs:500` | `%OFFSET` refused | §1 CL: `%OFFSET` | open |
| `lic/slic/src/ipl/mod.rs:260`, `net/tcpcfg/src/apply.rs:292` | no DHCP; `*IP4DHCP` interfaces refused | §1 DHCP client | open |
| `lic/slic/src/stgsvc/cmn/kernel.rs:178` | a kernel call kept for TCP/IP configuration (`dead_code`) | §1 DHCP client (its caller) | open |
| `work/qsjob/src/command/netcmd.rs:202` | `SNDTCPSPLF WSCST(name)` not read | §1 WSCST | open |
| `work/qsjob/src/command/ddm.rs:12` | DDM files do not open the remote file | §1 DDM | open |
| `work/qsjob/src/work/hostsvr/job.rs:313` | host servers return no spooled-file bytes | §1 host-server spooled bytes | open |
| `work/qsjob/src/work/telnet.rs:880` | telnet `INIT0100` automatic sign-on | §1 `INIT0100` | open |
| `lic/slic/src/ipl/media.rs:178`, `media/instmedia/src/rsp.rs:429` | `qs400.rsp=` URL; PXE | §1 response file over HTTP(S), PXE | open |
| `lic/slic/src/cpp/rpg/mod.rs:97`, `lic/slic/src/cpp/stmf.rs:40` | `INFOSTMF` PCML | §1 MIGRATE: PCML | open |
| `runtime/sysapi/src/ifs/names.rs:72` | a link's own owner (`lchown`) | §1 MIGRATE: `lchown` | open |
| `runtime/sysapi/src/ifs/refs.rs:135` | byte-range locks across jobs | §1 MIGRATE: byte-range locks | open |
| `lic/slic/src/cpp/lpcmd.rs:326` | product directories not removed by `DLTLICPGM` | §1 MIGRATE: product directories | open |
| `licpgm/lpstore/src/savfile.rs:156` | IFS directories in product streams | §1 MIGRATE: product stream directories | open |
| `licpgm/licpgm/src/api/crtpl.rs:191` | the directory list `SDIR0100` | §1 MIGRATE: `SDIR0100` | open |
| `work/qsjob/src/command/ftp.rs:12` | the `FTP` client's own session display | §1 MIGRATE: the FTP client display | open |
| `lic/slic/src/stgsvc/prtf.rs:84` | the printer device's queue as a printer file's default | §1 MIGRATE (phase 10 leftover) | open |
| `runtime/mi/src/job.rs:160` | a job's display (phase 8 leftover) | §1 MIGRATE: resolve or re-word | open |
| `work/qsjob/src/console/jobcmd.rs:113` | job commands shown through the service (phase 8) | §1 MIGRATE | open |
| `work/qsjob/src/display/surface.rs:7` | the console as a TN5250 client (phase 8) | §1 MIGRATE | open |
| `work/qsjob/src/work/inter.rs:9` | `*MENU` objects in Command Entry (phase 8) | §1 MIGRATE | open |
| `work/qsjob/src/work/sysjob.rs:14` | device arbitration, communications (phases 6, 10) | §1 MIGRATE | open |
| `work/wm/src/ctl.rs:243`, `work/wm/src/panels.rs:8` | display files for the job control panels (phase 8) | §1 MIGRATE | open |
| `runtime/sysapi/src/misc.rs:8` | commands through the analyzer (phase 7) | §1 MIGRATE | open |
| `runtime/qir/src/module/mod.rs:192,194` | `*DSP`/`*PRT` file kinds (`TODO(phase8)`) | §1: resolve or re-word | open |
| `work/qsjob/src/panels/system.rs:9` | `DSPSGNINF` (`TODO(phase8)`) | §1 | open |
| `lic/release/src/ptf.rs:6` | DSPPTF, GO LICPGM, QSZRTVPR (`TODO(phase11)`, built in Phase 11) | §1: comment is stale | open |

### "Not available in this release" sites

| Where | What | Closes / stays | State |
|---|---|---|---|
| `lic/slic/src/cpp/cmncmd.rs:285` | `VRYCFG STATUS` other than `*ON`/`*OFF` | stays: IBM's other statuses (`*RESET`, `*FRCOFF`...) need devices QS/400 does not have; re-checked in §1 | open |
| `lic/slic/src/cpp/direct.rs:198` | `GO` of a menu that does not exist | stays: IBM's own `CPD9D61` path | open |
| `lic/slic/src/cpp/medcmd.rs:167` | optical media libraries | stays: deferred in Phase 11 (no hardware) | open |
| `lic/slic/src/cpp/medcmd.rs:264` | `IMPORT(*YES)` on image catalogs | §1 triage | open |
| `lic/slic/src/cpp/medcmd.rs:320` | images from an optical device | §1 triage | open |
| `lic/slic/src/cpp/tcpcmd.rs:613,659` | `MRGTCPHT FROMOBJ`/`TOOBJ` stream files | §1 triage (stream files exist since Phase 10) | open |
| `lic/slic/src/stgsvc/svcop.rs:161`, `lic/svctools/src/env.rs:592` | copying a dump to media | §1 triage (media exist since Phase 11) | open |
| `lic/svctools/src/dst/mod.rs:22` | DST options not built (`DST.md`) | §1 triage per option | open |
| `runtime/msgf/msgs/qcpfmsg.toml`, `ui/menus/src/unavailable.rs` | the messages themselves (`CPD9D60`, `CPD9D61`...) | stays: the mechanism | open |

### Markers in documents

| Where | What | Closes / stays | State |
|---|---|---|---|
| `CLAUDE.md:42` | describes the markers | §0.2 | open |
| `ARCHITECTURE.md:134` | `TODO(phase11)` for DSPSFWRSC etc. (built) | §0.2/§2.4 | open |
| `DB_FORMAT.md:198` | `TODO(phase3)`: DSPFFD of kept attributes | §1 triage | open |
| `DDM.md:264` | DDM deviations `BLOCKED(phase10)` | §1 DDM | open |
| `INSTALL.md:188`, `LICPGM.md:492` | `qs400.rsp=` URL; `MIGRATE(phase11)` | §1 | open |
| `PHASE5.md`, `PHASE6.md`, `PHASE8.md`, `WORKMGMT.md`, `TOOLS.md` | historical markers in phase records | §1: annotated "closed in Phase N" | open |
| `PHASE9.md:199,218,260,268` | `DATA-INTO`; HTTPAPI over TLS (`QSOSSLSR`); pointer to a by-reference parameter; by-reference vs `* value` through a procedure pointer | §1 RPG; the last two are ABI changes, see below | open |

The two ABI items from `PHASE9.md` (a pointer taken to a by-reference
parameter dying with the entry; by-reference and `* value` not
interchangeable through a procedure pointer) are changes of the call
ABI (`ABI.md` §3). They are triaged in §1; if the change does not fit
before the freeze they stay as `BLOCKED(phase12)` with that reason,
since §2 freezes the module format the ABI lives in.

### Tests ignored for want of a golden

The brief counts 13; the tree at `93c6350f` has 20 (all in `ui/uim/tests/`).
Since 2026-10-31 the primary oracle answers telnet as `QSECOFR`, so the
ones pub400 refused can be captured there (V7R1M0).

| Test | Why it had none | Closes | State |
|---|---|---|---|
| `object_panels::dspfd` | no capture | §1 goldens | open |
| `object_panels::dspdtaara` | no capture | §1 goldens | open |
| `system2_panels::dspusrprf_grpmbr` | the pub400 account is not a group profile | §1 goldens (primary) | open |
| `system2_panels::edtautl` | no authorization list on pub400 | §1 goldens (primary) | open |
| `system2_panels::dspmod`, `dspmod_2` | no module the account may display | §1 goldens (primary) | open |
| `system2_panels::dspaudjrne` | needs `*AUDIT` | §1 goldens (primary) | open |
| `system2_panels::chgsysval`, `chgsysval_list` | `WRKSYSVAL` refused on pub400 | §1 goldens (primary) | open |
| `system_panels::wrksysval`, `wrkusrprf`, `wrkcfgsts`, `dspdevd`, `dspautl`, `wrkautl`, `dspsecaud` | no capture | §1 goldens | open |
| `wm_panels::wrkoutq`, `wrkjobq`, `wrkwtr`, `wrkjobscde` | no capture | §1 goldens | open |

### Open questions

| Document | Question | Closes / stays | State |
|---|---|---|---|
| `DB_OPEN_QUESTIONS.md` (Utilities) | what an RPG program sees in a `*USA` time field's buffer | §1: an RPG program on pub400 | open |
| `DB_OPEN_QUESTIONS.md` (Commitment control) | `ENDCMTCTL` with files open (the test on the machine) | §1: CL on the primary | open |
| `NET_OPEN_QUESTIONS.md` (IFS 8) | a binary read after a binary write to a member; QSH writes to a data member | §1: the primary | open |
| `NET_OPEN_QUESTIONS.md` (hwrsrc 1, 2) | `DSPHDWRSC` panels; the detail area below the location | §1: the primary as `QSECOFR` | open |
| `SEC_OPEN_QUESTIONS.md` | per-command IDs for a missing special authority | §1: the primary | open |
| `SVC_OPEN_QUESTIONS.md` 18 | what IBM mounts after `ENDOPT(*UNLOAD)` with no other entry loaded | stays: the oracles may not use tape | open |
| `SVC_OPEN_QUESTIONS.md` 19, 20 | real drive types; a real library's changer | `NEEDS-CODY`: physical tape acceptance | open |
| `SVC_OPEN_QUESTIONS.md` 21 | `OVRTAPF`; copy message IDs; `CPYFRMTAP OUTFMT` on the first page | §1 triage | open |
| `SVC_OPEN_QUESTIONS.md` 28 | refusing `SAVSYS` outside restricted state | §1: after the `ENDSBS *ALL` fix | open |
| `SVC_OPEN_QUESTIONS.md` 32 | `CHKOBJITG OUTFILE` in IBM's layout | §1: `DSPFFD` on the primary | open |
| `SVC_OPEN_QUESTIONS.md` 33 | the recovery's `RST` leaving 7 links | §1 | open |
| `UI_OPEN_QUESTIONS.md` 4, 6, 8, 12 | start-up response record; keys that carry data; the printer data stream; SST's audit code | §1: the primary's telnet | open |

## Inventory

| Commit | What |
|---|---|
| `0f7d4df1` | this file; `CLAUDE.md` rewritten for Phases 0 to 11 and every crate |
| `de57ebda` | `tools/fmtcheck`: 23 frozen formats, their lock, `build test` runs it; "Frozen at V1R1M0" in ten format documents; version constants for the PTF header, `QAUDJRN` layouts, authority structures, host-server codec and oracle fixtures |
| (next) | `RELEASE` is V1R1M0; install step 10 installs V1R0M0 media and upgrades it with user data; the kernel's drivers (§6); `tools/fuzz` and `build fuzz` (§9) |

## Format freeze (Section 2)

`tools/fmtcheck/formats.toml` names every frozen format: the object
store (pages, superblock, units, WAL, B+tree nodes, allocation, parity,
the IFS map, the object header and the store's own payloads), the LIC
partition, the database and journal payloads, the save stream and its
outfiles, PTF packages, the update protocol, product payloads, virtual
tape and the media labels, QAP versions 1 to 6, the host-server codec,
spooled files and the other work management payloads, `QAUDJRN`
layouts, `*USRPRF`, the stored authority structures, ILE objects and the
IR's binary module, the command, message, display file, panel group and
menu payloads, IFS metadata, communications, registration facility and
certificate store payloads, and the oracle fixture formats. The hash is
over Rust tokens (comments, formatting and test items do not count).
`cargo test -p fmtcheck` and `build test` fail on a frozen definition
changed without its version constant; `cargo run -p fmtcheck -- update`
records a bump.

## Deviations

- **The fuzzer is ours.** `cargo fuzz` runs libFuzzer, which is C++
  compiled by `libfuzzer-sys`'s build script; that breaks the pure-Rust
  rule. `tools/fuzz` (`qsfuzz`) is coverage-guided the same way: the
  build passes LLVM's SanitizerCoverage flags (`-C passes=sancov-module`,
  inline 8-bit counters), and `tools/fuzz/src/cov.rs` receives the
  counter arrays and reads them after every input. Mutations, corpus,
  crash minimisation and a hang watchdog are in `engine.rs` and
  `mutate.rs`. `build fuzz` builds it instrumented and runs each target.
- **Console drivers.** The console stays on the firmware's framebuffer
  through `simpledrm` (`CONSOLE.md`): Intel, AMD, Matrox G200 (iDRAC,
  iLO) and ASPEED BMC graphics all hand over a UEFI GOP framebuffer, so
  no native DRM driver is built. Native drivers would modeset, which
  QS/400 does not do, and i915/amdgpu need firmware files a
  `MODULES=n` kernel with no firmware in the initramfs cannot load.

## NEEDS-CODY

Exact steps are filled in as each item is reached.

| # | Item | Steps | State |
|---|---|---|---|
| 1 | ACS compatibility checklist for the host servers | `PHASE10.md` "ACS checklist (Cody)", seven checks, on a V1R1M0 install | open |
| 2 | HLINV: the two printer files using type `Y`; keystroke scripts for the interactive diffs | | open |
| 3 | The pub400 library name in commit `f994a61` | confirm it is not sensitive (history is not rewritten) | open |
| 4 | `rpg-oracle clean` on `QSTEST` | | open |
| 5 | Physical tape acceptance | `MEDIA.md` "Tape: physical tape acceptance" | open |
| 6 | `QSHWQUAL` on the Dell with the PERC H330 | `QUALIFIED_HARDWARE.md` | open |
| 7 | Goldens that need a command run on the primary | | open |
| 8 | `QRPG` on pub400 or the primary for Section 8 | | open |
| 9 | (dropped: the 72-hour soak; Phase 12 runs a one-hour soak) | | closed |

## Acceptance

| Criterion | State |
|---|---|
| Zero `BLOCKED(phase9|10|11)` and zero `MIGRATE(...)` markers in the tree | |
| Zero tests ignored for "no golden" | |
| `fmtcheck` passes; slip-install V1R0M0 → V1R1M0 preserves all user data | |
| `RELEASE == V1R1M0` everywhere a version is shown | |
| QAPM* files diff clean against the oracle; collector produces consistent data | |
| `helpaudit` 100% panels and commands; `textaudit` zero literals | |
| `INSLNG` installs the `QSYS2931` scaffold; secondary-language sign-on works | |
| `QSHWQUAL` runs from media under QEMU; qualified-hardware doc written | |
| QAP spec frozen; guest agent recognised by QEMU as a guest agent | |
| 20 RPG III programs conformant; `CVTRPGSRC` report diffs clean | |
| All fuzz targets 3 minutes crash-free, 6 at once (Rust fuzzer, see Notes; shortened from 30 min at Cody's request) | Pass 2026-11-03: 18 targets, 0.26M-1.13M execs each, 0 crashes (two crashes found earlier and fixed: `02ed74f3`, `7eb1c250`) |
| `QSSECCHK` clean on a fresh install | |
| Soak clean: one-hour run (the 72-hour run dropped at Cody's request) | |
| Knowledge Center builds and is served; release notes written | |
| `build release` produces a signed, bootable, installable ISO | |
| `CLAUDE.md` accurately describes the system as shipped | |
