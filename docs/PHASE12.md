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
Section 2 has landed. They ship on the V1R1M0 media all the same
(QS00DG1 HTTP Server, QS00NAV Navigator; QS00AFW stays reserved), so
after the freeze their tests ran on the frozen tree (the release
relabelled their footers, and a fixture account was renamed for the
hygiene rule) and their QEMU steps 64, 70 and 86 to 88 ran with the
release verification.

## Carried items

The SQL refusals are not repeated here: `SQL.md` §10 is their one list, and the
release notes import it ("Refused in SQL"), so the two cannot diverge. The
live marker inventory is the release notes' generated "Marker inventory".

One row per `BLOCKED`, `MIGRATE` and `TODO` marker, per "not available
in this release" site, per test ignored for want of a golden and per
open question, as found at `93c6350f`. "Closes" names the Phase 12 item
that removes it; "Stays" says why it remains. The State column is
updated as rows close.

### Markers in code

| Where | What | Closes / stays | State |
|---|---|---|---|
| `rpg/compiler/src/files.rs:499` | `LIKEFILE` parameters (a file passed to a procedure) | §1 RPG: `LIKEFILE` | closed |
| `rpg/compiler/src/codegen/xml.rs:303` | `DATA-INTO`/`DATA-GEN` call no parser or generator | §1 RPG: `DATA-INTO`/`DATA-GEN` | closed |
| `rpg/compiler/src/codegen/dbio.rs:153` | Open Access handlers are not called | §1 RPG: Open Access | closed |
| `rpg/qrpgrt/src/ctl.rs:41` | `DUMP(A)` writes no formatted dump | §1 RPG: `DUMP(A)` | closed |
| `command/cl/src/lower/expr.rs:500` | `%OFFSET` refused | §1 CL: `%OFFSET` | closed |
| `lic/slic/src/ipl/mod.rs:260`, `net/tcpcfg/src/apply.rs:292` | no DHCP; `*IP4DHCP` interfaces refused | §1 DHCP client | closed |
| `lic/slic/src/stgsvc/cmn/kernel.rs:178` | a kernel call kept for TCP/IP configuration (`dead_code`) | §1 DHCP client (its caller) | closed |
| `work/qsjob/src/command/netcmd.rs:202` | `SNDTCPSPLF WSCST(name)` not read | §1 WSCST | closed |
| `work/qsjob/src/command/ddm.rs:12` | DDM files do not open the remote file | §1 DDM | closed |
| `work/qsjob/src/work/hostsvr/job.rs:313` | host servers return no spooled-file bytes | §1 host-server spooled bytes | closed |
| `work/qsjob/src/work/telnet.rs:880` | telnet `INIT0100` automatic sign-on | §1 `INIT0100` | closed |
| `lic/slic/src/ipl/media.rs:178`, `media/instmedia/src/rsp.rs:429` | `qs400.rsp=` URL; PXE | §1 response file over HTTP(S), PXE | closed |
| `lic/slic/src/cpp/rpg/mod.rs:97`, `lic/slic/src/cpp/stmf.rs:40` | `INFOSTMF` PCML | §1 MIGRATE: PCML | closed |
| `runtime/sysapi/src/ifs/names.rs:72` | a link's own owner (`lchown`) | §1 MIGRATE: `lchown` | closed |
| `runtime/sysapi/src/ifs/refs.rs:135` | byte-range locks across jobs | §1 MIGRATE: byte-range locks | closed |
| `lic/slic/src/cpp/lpcmd.rs:326` | product directories not removed by `DLTLICPGM` | §1 MIGRATE: product directories | closed |
| `licpgm/lpstore/src/savfile.rs:156` | IFS directories in product streams | §1 MIGRATE: product stream directories | BLOCKED(rel): SAVLICPGM writes no IFS records, RSTLICPGM restores none |
| `licpgm/licpgm/src/api/crtpl.rs:191` | the directory list `SDIR0100` | §1 MIGRATE: `SDIR0100` | closed |
| `work/qsjob/src/command/ftp.rs:12` | the `FTP` client's own session display | §1 MIGRATE: the FTP client display | closed |
| `lic/slic/src/stgsvc/prtf.rs:84` | the printer device's queue as a printer file's default | §1 MIGRATE (phase 10 leftover) | closed |
| `runtime/mi/src/job.rs:160` | a job's display (phase 8 leftover) | §1 MIGRATE: resolve or re-word | closed |
| `work/qsjob/src/console/jobcmd.rs:113` | job commands shown through the service (phase 8) | §1 MIGRATE | closed (reworded); another job's call stack is BLOCKED(rel) at jobcmd.rs:153 |
| `work/qsjob/src/display/surface.rs:7` | the console as a TN5250 client (phase 8) | §1 MIGRATE | closed |
| `work/qsjob/src/work/inter.rs:9` | `*MENU` objects in Command Entry (phase 8) | §1 MIGRATE | closed |
| `work/qsjob/src/work/sysjob.rs:14` | device arbitration, communications (phases 6, 10) | §1 MIGRATE | closed |
| `work/wm/src/ctl.rs:243`, `work/wm/src/panels.rs:8` | display files for the job control panels (phase 8) | §1 MIGRATE | closed |
| `runtime/sysapi/src/misc.rs:8` | commands through the analyzer (phase 7) | §1 MIGRATE | closed |
| `runtime/qir/src/module/mod.rs:192,194` | `*DSP`/`*PRT` file kinds (`TODO(phase8)`) | §1: resolve or re-word | closed |
| `work/qsjob/src/panels/system.rs:9` | `DSPSGNINF` (`TODO(phase8)`) | §1 | closed |
| `lic/release/src/ptf.rs:6` | DSPPTF, GO LICPGM, QSZRTVPR (`TODO(phase11)`, built in Phase 11) | §1: comment is stale | closed |

### "Not available in this release" sites

| Where | What | Closes / stays | State |
|---|---|---|---|
| `lic/slic/src/cpp/cmncmd.rs:285` | `VRYCFG STATUS` other than `*ON`/`*OFF` | stays: IBM's other statuses (`*RESET`, `*FRCOFF`...) need devices QS/400 does not have; re-checked in §1 | stays: IBM's other statuses need devices QS/400 does not have |
| `lic/slic/src/cpp/direct.rs:198` | `GO` of a menu that does not exist | stays: IBM's own `CPD9D61` path | stays: IBM's own CPD9D61 path |
| `lic/slic/src/cpp/medcmd.rs:167` | optical media libraries | stays: deferred in Phase 11 (no hardware) | stays: deferred in Phase 11 (no hardware) |
| `lic/slic/src/cpp/medcmd.rs:264` | `IMPORT(*YES)` on image catalogs | §1 triage | closed |
| `lic/slic/src/cpp/medcmd.rs:320` | images from an optical device | §1 triage | closed |
| `lic/slic/src/cpp/tcpcmd.rs:613,659` | `MRGTCPHT FROMOBJ`/`TOOBJ` stream files | §1 triage (stream files exist since Phase 10) | closed |
| `lic/slic/src/stgsvc/svcop.rs:161`, `lic/svctools/src/env.rs:592` | copying a dump to media | §1 triage (media exist since Phase 11) | closed |
| `lic/svctools/src/dst/mod.rs:22` | DST options not built (`DST.md`) | §1 triage per option | closed in the pre-release round: every option opens a panel or says why it does not apply; save/restore storage, one unit's data and Save LIC stay `BLOCKED(rel)` (`lic/svctools/src/dst/answers.rs`) |
| `runtime/msgf/msgs/qcpfmsg.toml`, `ui/menus/src/unavailable.rs` | the messages themselves (`CPD9D60`, `CPD9D61`...) | stays: the mechanism | stays: the mechanism |

### Markers in documents

| Where | What | Closes / stays | State |
|---|---|---|---|
| `CLAUDE.md:42` | describes the markers | §0.2 | closed (0f7d4df1) |
| `ARCHITECTURE.md:134` | `TODO(phase11)` for DSPSFWRSC etc. (built) | §0.2/§2.4 | closed |
| `DB_FORMAT.md:198` | `TODO(phase3)`: DSPFFD of kept attributes | §1 triage | closed |
| `DDM.md:264` | DDM deviations `BLOCKED(phase10)` | §1 DDM | closed |
| `INSTALL.md:188`, `LICPGM.md:492` | `qs400.rsp=` URL; `MIGRATE(phase11)` | §1 | closed |
| `PHASE5.md`, `PHASE6.md`, `PHASE8.md`, `WORKMGMT.md`, `TOOLS.md` | historical markers in phase records | §1: annotated "closed in Phase N" | closed |
| `PHASE9.md:199,218,260,268` | `DATA-INTO`; HTTPAPI over TLS (`QSOSSLSR`); pointer to a by-reference parameter; by-reference vs `* value` through a procedure pointer | §1 RPG; the last two are ABI changes, see below | closed: DATA-INTO in Phase 12; TLS, headers and the ABI items re-tagged BLOCKED(rel) (b855a775) |

The two ABI items from `PHASE9.md` (a pointer taken to a by-reference
parameter dying with the entry; by-reference and `* value` not
interchangeable through a procedure pointer) are changes of the call
ABI (`ABI.md` §3). They are triaged in §1; if the change does not fit
before the freeze they stay as `BLOCKED(rel)` with that reason,
since §2 freezes the module format the ABI lives in.

### Tests ignored for want of a golden

The brief counts 13; the tree at `93c6350f` has 20 (all in `ui/uim/tests/`).
Since 2026-10-31 the primary oracle answers telnet as `QSECOFR`, so the
ones pub400 refused can be captured there (V7R1M0).

| Test | Why it had none | Closes | State |
|---|---|---|---|
| `object_panels::dspfd` | no capture | §1 goldens | closed |
| `object_panels::dspdtaara` | no capture | §1 goldens | closed |
| `system2_panels::dspusrprf_grpmbr` | the pub400 account is not a group profile | §1 goldens (primary) | closed |
| `system2_panels::edtautl` | no authorization list on pub400 | §1 goldens (primary) | closed |
| `system2_panels::dspmod`, `dspmod_2` | no module the account may display | §1 goldens (primary) | closed |
| `system2_panels::dspaudjrne` | needs `*AUDIT` | §1 goldens (primary) | NEEDS-CODY: the primary has no QAUDJRN |
| `system2_panels::chgsysval`, `chgsysval_list` | `WRKSYSVAL` refused on pub400 | §1 goldens (primary) | closed |
| `system_panels::wrksysval`, `wrkusrprf`, `wrkcfgsts`, `dspdevd`, `dspautl`, `wrkautl`, `dspsecaud` | no capture | §1 goldens | closed |
| `wm_panels::wrkoutq`, `wrkjobq`, `wrkwtr`, `wrkjobscde` | no capture | §1 goldens | closed |

### Open questions

| Document | Question | Closes / stays | State |
|---|---|---|---|
| `DB_OPEN_QUESTIONS.md` (Utilities) | what an RPG program sees in a `*USA` time field's buffer | §1: an RPG program on pub400 | settled on pub400; the buffer form BLOCKED(rel) |
| `DB_OPEN_QUESTIONS.md` (Commitment control) | `ENDCMTCTL` with files open (the test on the machine) | §1: CL on the primary | closed |
| `NET_OPEN_QUESTIONS.md` (IFS 8) | a binary read after a binary write to a member; QSH writes to a data member | §1: the primary | QSH half closed; the binary read NEEDS-CODY (a JTOpen probe) |
| `NET_OPEN_QUESTIONS.md` (hwrsrc 1, 2) | `DSPHDWRSC` panels; the detail area below the location | §1: the primary as `QSECOFR` | 1 closed (DSPHDWRSC panels); 2 stays open |
| `SEC_OPEN_QUESTIONS.md` | per-command IDs for a missing special authority | §1: the primary | NEEDS-CODY: needs a test user profile on the primary |
| `SVC_OPEN_QUESTIONS.md` 18 | what IBM mounts after `ENDOPT(*UNLOAD)` with no other entry loaded | stays: the oracles may not use tape | stays: the oracles may not use tape |
| `SVC_OPEN_QUESTIONS.md` 19, 20 | real drive types; a real library's changer | `NEEDS-CODY`: physical tape acceptance | NEEDS-CODY: physical tape acceptance |
| `SVC_OPEN_QUESTIONS.md` 21 | `OVRTAPF`; copy message IDs; `CPYFRMTAP OUTFMT` on the first page | §1 triage | stays open: not reached in Phase 12 |
| `SVC_OPEN_QUESTIONS.md` 28 | refusing `SAVSYS` outside restricted state | §1: after the `ENDSBS *ALL` fix | closed: SAVSYS refuses outside the restricted state (CPF3785) |
| `SVC_OPEN_QUESTIONS.md` 32 | `CHKOBJITG OUTFILE` in IBM's layout | §1: `DSPFFD` on the primary | closed: QASYCHKI layout, CHKOBJITG OUTFILE |
| `SVC_OPEN_QUESTIONS.md` 33 | the recovery's `RST` leaving 7 links | §1 | stays open: not reached in Phase 12 |
| `UI_OPEN_QUESTIONS.md` 4, 6, 8, 12 | start-up response record; keys that carry data; the printer data stream; SST's audit code | §1: the primary's telnet | 4 closed; 6 open (a display-file probe); 8 and 12 NEEDS-CODY |

## Inventory

| Commit | What |
|---|---|
| `0f7d4df1` | this file; `CLAUDE.md` rewritten for Phases 0 to 11 and every crate |
| `de57ebda` | `tools/fmtcheck`: 23 frozen formats, their lock, `build test` runs it; "Frozen at V1R1M0" in ten format documents; version constants for the PTF header, `QAUDJRN` layouts, authority structures, host-server codec and oracle fixtures |
| `24d2be6c`, `73309c30` | the kernel's drivers (§6); `RELEASE` is V1R1M0, install step 10 installs V1R0M0 media and upgrades it with user data |
| `c0b679b1`..`18df4491` | `tools/fuzz` (`qsfuzz`) and `build fuzz`; two crashes it found fixed (RPG reader, SQL nesting `SQL0101`); the corpora |
| `bdb100b1` | `tools/kcbuild`, the Knowledge Center |
| `febab6a3`, `16024e0a` | §1 DDM (server on 446, DDM files, remote RLA, server authentication entries) and `*WSCST` (tag language, the 153 shipped objects); QEMU 100, 101 |
| `add7abc6`..`dd5b7698` | §1 RPG: `LIKEFILE`, `DATA-INTO`/`DATA-GEN` with shipped parsers and `QOAR` members, Open Access, `DUMP(A)` (`QPPGMDMP`); batches b17-b21 |
| `554dfad1`..`cc0e5e61` | §1 CL `%OFFSET`, `ENDSBS *ALL` (restricted state, `SAVSYS` refused outside it), the MIGRATE and TODO markers, the "not available" sites; QEMU 103-105 |
| `d0e9c79e`..`e3bb6e57` | §1 the "no golden" tests (19 captured on the primary), the layout ignores, the open questions the primary answers |
| `d5ae3d31`..`34ed14e9` | §1 DHCPv4 client (`net/dhcp`), the response file over HTTP(S)/TFTP/PXE, telnet `INIT0100`, spooled bytes from the print server, SMTP relay credentials; QEMU 96-98, install 15 |
| `6dc772b6`..`e66aabe3` | §3 Collection Services, `*MGTCOL`, `CRTPFRDTA`, the QAPM files, `WRKSYSACT` under QS00PT1, `QPFRADJ`; QEMU 106 |
| `349f9db8`..`0244f54d` | §4 help everywhere: `tools/helpaudit` (100%), command and parameter help, DDS help, every message's second-level text, F1/F2 at the prompter, `*SCHIDX`, `QHSS1`, `DSPHLPDOC`; help in the Knowledge Center |
| `730240f9`..`aa24dfbe` | §8 QS00RPG: `CRTRPGPGM`, `CVTRPGSRC`, batch b22rpg3 (26 programs), `RPG3.md` |
| `e3ce0bda`..`1834615c`, `89e3391d`..`c3e231bf` | §6, §7: `lic/hwqual`, `QSHWQUAL` (command, media option 4), `QUALIFIED_HARDWARE.md`, `lic/guestagent` (QEMU guest agent protocol, vsock), QAP version 7 `Manage` (draft) in `qap-proto`, `qapd` and the connection job; QEMU 114-117, 123, media 5 |
| `ed5876ab`..`8dd6c0d8` | `tools/textaudit` (baseline), `QSTXTMSG`, CCSIDs kept on objects, `INSLNG`, `QSYS2924`, the `QSYS2931` scaffold, one format path for dates, times and decimals; QEMU 120-122 |
| `1f4b01d6`..`06c0ff60` | `QSSECCHK`, job accounting (`work/acg`, `QACGJRN`, `CHGACGCDE`), `build soak`; the IPL narrowing of `*PUBLIC` on shipped objects; QEMU 107, 108 |
| `8e8d6cd5` | problem reports to the update service (`*PRB`), the last `MIGRATE` marker |
| `52bf54c6` | `build kc`, `build release` (§10, §11) |
| `d50f4904`.., `3d204664` | `CHGSYSVAL` of a blank value, problem log and PTF spaces `*PUBLIC *USE`, `DSPLIBL *PRINT` spooled; the prestart idle count and the page cache sized from `QMCHPOOL` (the soak's leaks); `procstat` names jobs |
| `12d1e338`, `3d1cee0c` | the image compressed once with xz: the media UKI from 40.6 to 25.8 MiB; `build iso` refuses a boot image over El Torito's limit |

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

- **Adding an object type is not a format change.** A new type code in `storage/store/src/obj/types.rs` (`*MGTCOL`, 0x002C) moves the frozen `store` hash; existing stores read the same, and a V1R0M0 system never opens a V1R1M0 store (the release rule refuses the downgrade). Such additions are recorded with `fmtcheck update --force` and named here; a change to an existing layout still needs the version bump.
- **Pre-release format records (`fmtcheck update --force`).** None changes a stored layout an existing system reads differently: the superblock's reserved bytes 200..256 hold the hot spares (`spare_count`, `spares`) and byte 201 the ASP threshold, zero meaning none and 90% (every store written so far has zeros there); `sql-info` gained entry tag 19 `TABLE` (`VOLATILE`; a reader skips tags it does not know); `*TIMZON` is a new object type (0x002E) with a new payload, `timzon` version 1 in `wm-objects`.
- **The V1R1M0 reformat.** The whole tree was formatted once with rustfmt after the last merge (`rustfmt` gates CI since). rustfmt reorders `use` lists and adds trailing commas, which moves fmtcheck's token hashes without changing a layout; the ten formats it moved were re-locked in the same commit, after checking that the tree before it passed.
- **The host-server codec decodes two more frame kinds.** The central server's end job (`0x1400`) and the native database server's frames (`0xE005`) were in the recorded traces but had no name; `net/hostsvr`'s frozen codec now types them. Nothing on the wire changed, so no version bump (`fmtcheck update --force`).
- **fmtcheck reads LF.** A Windows checkout holds CR LF, which a string literal spanning lines carries into its token: the lock written on Windows disagreed with Linux for `update-protocol`. fmtcheck now hashes every source with LF line ends.
- **The release image is UEFI only and compressed once with xz.** The kernel is an EFI-stub UKI (`ARCHITECTURE.md` §5), so there is no BIOS boot; the initramfs is embedded uncompressed and the whole image is xz, so the media UKI (25.8 MiB) fits the 32 MiB an El Torito boot image can name.
- **The page cache follows `QMCHPOOL`.** The cache was sized before the system values were loaded and kept every page it touched up to 256 MiB; it now takes `QMCHPOOL` (20,000 KB shipped) once they are loaded.
- **`textaudit` ships with a baseline.** 3,791 literals on display paths remain (developer tools, service listings, job panels); the count may only go down (Cody's choice, 2026-11-04).
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

| # | Item | Steps | State |
|---|---|---|---|
| 1 | ACS compatibility checklist for the host servers | `PHASE10.md` "ACS checklist (Cody)": the seven checks, on a V1R1M0 install (ISO in `Desktop\QS400-V1R1M0-release-devkey\`) | open |
| 2 | HLINV | the two printer files with data type `Y` (IBM refuses them too: decide whether to change the source), and keystroke scripts for the interactive diffs (`PHASE9.md` Acceptance 2) | open |
| 3 | The pub400 library name in commit `f994a61` | confirm it is not sensitive; history is not rewritten. The account name in the web fixtures is now `TESTUSER` (`125c935d`) | open |
| 4 | `rpg-oracle clean` | done on pub400 by the RPG agents (Phase 12); on the primary `QSTEST` holds only `QSMSGSRC` and `QSMSGDBG`, which predate Phase 12: confirm they are yours | open |
| 5 | Physical tape acceptance | `MEDIA.md` "Tape: physical tape acceptance"; settles `SVC_OPEN_QUESTIONS.md` 19 and 20 | open |
| 6 | `QSHWQUAL` on the Dell with the PERC H330 and the hypervisors | `QUALIFIED_HARDWARE.md` "NEEDS-CODY": Proxmox, ESXi, Workstation, VirtualBox, Hyper-V Gen2 and the Dell checklists; also try booting the ISO as a CD on real firmware | open |
| 7 | Oracle commands that need a configuration change on the primary | `SEC_OPEN_QUESTIONS.md` (a `QAUDJRN` for the `DSPAUDJRNE` golden and SST's audit code; a test profile for the per-command special-authority IDs), `UI_OPEN_QUESTIONS.md` 8 (a printer device), `NET_OPEN_QUESTIONS.md` IFS 8 (the JTOpen probe of a binary read) | open |
| 8 | `QRPG` for Section 8 | closed: pub400 has `CRTRPGPGM`, `CVTRPGSRC`, `QRPG` and `QRPG38`; every RPG III result is pub400's | closed |
| 9 | The 72-hour soak | dropped at Cody's request; a one-hour soak runs instead | closed |
| 10 | Sign the release | `QS400_SIGNING_KEY=<the release key> cargo run -p build -- release`; the copy on the Desktop is signed with a development key (public key `48c829c2...`) | open |
| 11 | Two judgement calls | `QSSECCHK` treats `*CHANGE` on device, controller and line descriptions as right (IBM ships them so); job accounting writes no `DP` entries (every printer file spools; a printer file's close adds to the job's `JB`) | open |
| 12 | Tag `V1R1M0` | after 10, on the commit the release was built from | open |

## Acceptance

The state on the final tree (`3d1cee0c` and its last fixes), 2026-11-04.
QEMU runs are KVM guests in WSL on the development PC.

| Criterion | State |
|---|---|
| Zero `BLOCKED(phase9|10|11)` and zero `MIGRATE(...)` markers in the tree | Pass: none outside this file's history table |
| Zero tests ignored for "no golden" | Pass: 19 captured on the primary and un-ignored; `dspaudjrne` is `NEEDS-CODY` (the primary has no `QAUDJRN`) |
| `fmtcheck` passes; slip-install V1R0M0 → V1R1M0 preserves all user data | Pass: `fmtcheck` in `build test` and `cargo test`; install step 10 installs V1R0M0 media, makes a data area, a journaled table with entries, spooled files and a `SAVLIB` in a save file, upgrades with V1R1M0, finds all of it, restores the V1R0M0 save and refuses the downgrade |
| `RELEASE == V1R1M0` everywhere a version is shown | Pass: `lic/release` (`73309c30`); no other release literal outside tests |
| QAPM* files diff clean against the oracle; collector produces consistent data | Pass: `work/perf/tests/ffd.rs` (601 fields of the eight files against the primary's `DSPFFD`); QEMU 106 |
| `helpaudit` 100% panels and commands; `textaudit` zero literals | Partly: `helpaudit` 100% in every category, in `build test`. `textaudit` ships and fails on any new literal; 3,791 sites remain on its baseline (Cody chose to ship the baseline) |
| `INSLNG` installs the `QSYS2931` scaffold; secondary-language sign-on works | Pass: QEMU 120 and 121 (`INSLNG`'s prompt has no oracle: V7R1 has no `INSLNG`) |
| `QSHWQUAL` runs from media under QEMU; qualified-hardware doc written | Pass: media step 5, QEMU 114; `QUALIFIED_HARDWARE.md` (the other platforms `NEEDS-CODY`) |
| QAP spec frozen; guest agent recognised by QEMU as a guest agent | Pass: versions 1 to 6 frozen, version 7 (`Manage`) draft; QEMU 115 to 117 (QEMU's guest-agent channel answers `guest-ping`, `guest-info`, `guest-shutdown`), 123 (`Manage` end to end) |
| 20 RPG III programs conformant; `CVTRPGSRC` report diffs clean | Pass: 26 programs, 25 match and one known difference (R3021, `known.txt`); 26 of 26 conversions and reports match pub400's |
| All fuzz targets crash-free (3 minutes each, shortened at Cody's request) | Pass: 18 targets, 0.26M to 1.13M execs each; two crashes found earlier and fixed |
| `QSSECCHK` clean on a fresh install | Pass: QEMU 107; 23 checks, 17 pass, 6 warnings (IBM's shipped values and `QAUDCTL *NONE`, `SECURITY.md` §10), 0 failures |
| Soak clean (one hour, at Cody's request) | Pass (3,600 s on the final tree): 1,726 console cycles, 3,452 batch jobs, 345 save/restore rounds, 924 interactive rounds, 0 failures; the storage service 115 → 122 MB raw, net of its cache 15 → 11 MB, descriptors 81 → 73; every other process flat; disk +35 MiB; active jobs 56 → 48; no new SRC; `QSSECCHK` 0 failures; 5,105 accounting entries. The leaks the first soak found (prestart idle count, an unsized page cache) are fixed |
| Knowledge Center builds and is served; release notes written | Partly: `build kc` (980 pages) and on the media under `QS400/KC/`; served over HTTP only once QS00AFW ships. `RELEASE_NOTES_V1R1M0.md` written |
| `build release` produces a signed, bootable, installable ISO | Pass with a development key: the ISO boots under OVMF and its automatic install reaches IPL complete; the dev-build install suite passes steps 1 and 8 (without and with a response file). UEFI only; signing with the release key is `NEEDS-CODY` 10 |
| `CLAUDE.md` accurately describes the system as shipped | Pass: rewritten (`0f7d4df1`) and kept current with every merge |

QEMU on the merged tree (agents N and O, 2026-11-04): disk suite steps
1, 64, 68, 70, 80, 86 to 89, 100, 101, 103 to 108, 114 to 117, 120 to
123 pass; install suite steps 1, 8, 9 and 10 pass; install step 15
(the network install) fails after its install: the installed system
never gets a DHCP lease (`BLOCKED(rel)`).

## Exit review

| Phase | Acceptance today | Reopened or closed in Phase 12 |
|---|---|---|
| 0, 1 | stands | the kernel config: more drivers, the image compressed once with xz (`ARCHITECTURE.md` §6) |
| 2, 2.5 | stands | the store's frozen layouts; the page cache sized from `QMCHPOOL` after the system values load (it had kept every page) |
| 3 | stands | `ENDCMTCTL` with files open is `CPF8355`; SQL nested past 64 levels is `SQL0101` (found by the fuzzer) |
| 4 | stands | the IR and object formats frozen; the call ABI items stay `BLOCKED(rel)` |
| 5 | stands | `ENDSBS *ALL` reaches the restricted state; Collection Services; job accounting; the prestart idle count (a descriptor leak under load) |
| 6 | stands | `*PUBLIC *EXCLUDE` had been widened at the first IPL; `QSSECCHK` found it and system data areas changeable by any user |
| 7 | stands | `%OFFSET`; `CRTBNDCL` with default options; the wrapped command line did not reproduce |
| 8 | stands | 19 panel goldens on the primary; help on every panel; the renderer's key and option rules |
| 9 | stands; HLINV and the open-source items as before | `LIKEFILE`, `DATA-INTO`/`DATA-GEN`, Open Access, `DUMP(A)`; RPG III; a reader panic found by the fuzzer |
| 10 | stands; the ACS checklist still Cody's | DHCP, `INIT0100`, spooled bytes, SMTP credentials, DDM, `*WSCST`, the network response file; the network install itself blocked |
| 11 | stands | `SAVSYS` in restricted state; the upgrade from V1R0M0 proven end to end; problem reports to the update service |

Verification caveat: all RPG conformance (ILE RPG and RPG III) is
against pub400.com (IBM i V7R5M0); the primary oracle (V7R1M0) has no
RPG compiler. Screen goldens added in Phase 12 come from the primary
(V7R1M0) unless they say otherwise.

## Numbers

| Measure | Value |
|---|---|
| Commits in Phase 12 | 170 (`93c6350f`..) |
| Workspace crates | 114 |
| `#[test]` functions | 3,643 (54 ignored, each with its reason) |
| QEMU steps (disk suite) | 96 named steps |
| `unsafe` blocks | 104 in 19 files (84 outside tests); 105 `SAFETY` comments; the files are the ones `CLAUDE.md` lists |
| Dependency audit | `build deny` clean (332 crates, no C linkage); cargo-deny's advisories stand in for cargo-audit (same RustSec database) |
| Media UKI | 25.8 MiB (27,018,240 bytes), boot image 31 MiB |
| V1R1M0 ISO | `QS400-V1R1M0-c35f266c792f.iso`, 148,516,864 bytes, SHA-256 `b00008946fe2df8ac04818ed10c59581389f4b898be528ec0e857a9ceca6e9ed` (development key; PTF baseline `C26001110`) |

## Release gate

The pre-release handoff's acceptance list (2026-11-06/07). QEMU runs are
GitHub-hosted (`qemu.yml`) unless they say WSL; run IDs are GitHub Actions
runs.

| Criterion | State |
|---|---|
| `fmtcheck` passes with every frozen format at V1R1M0 and no V1R2M0 anywhere | Met: `sql-info` and `sql-package` frozen at V1R1M0 (018b721f); no `V1R2M0` in the tree; fmtcheck clean, hashing LF on every host |
| Slip-install V1R0M0 → V1R1M0 preserves data and old saves restore | Met: install step 10 (WSL, clean build) with V1R0M0 media from `becee24b` (object header and save stream version 1): data area, journaled SQL table and entries, spooled files kept; the V1R0M0 table used by SQL (SELECT, INSERT, index, view); a V1R0M0 `SAVLIB` restored; V1R0M0 media refused; after the product ID change `DSPSFWRSC` shows QS00 IDs and `CHKPRDOPT 5770SS1` works |
| One product ID family in code, tests, manifests and notes | Met: `QS00SS1`, `QS00WDS`, `QS00999` everywhere a product ID is shown; `5770*` accepted on input (`release::IBM_ALIASES`) and in stored V1R0M0 PTF records; oracle captures keep IBM's IDs; QEMU 80, 81 |
| Zero `CPD0790` sites; zero accepted-no-effect clauses outside the documented IBM no-ops | Met: `CPD0790` deleted with its six CL sites implemented; `VOLATILE` acts, `IN`/`UNIT`/`PAGESIZE` as Db2 for i; CGI and `*VLDL` refused (501/403); `BCHTIMLMT`, `SNDPGMMSG TOUSR`, `RCVMSG`'s ignored parameters and `SBMJOB USER(*CURRENT)` implemented; the system values with no effect are listed in the notes |
| QAP `*EXCLUDE` test passes; `QSSECCHK` clean on a fresh install | Met: `osobjects` `tests/exclude.rs` (command, program, library, after one and two IPLs); QEMU 123 (the refusals, a program without `*EXECUTE`, the QCMDEXC and CL paths) and 107 (0 failures, 6 warnings: IBM's shipped values) |
| QEMU steps 130+ exist, run, and the `SQL.md` step table is written | Met: steps 130 to 139 pass (runs 42316635829, 42316642611, 42316646039, 42323304005, 42323333123), 139 between two guests; `SQL.md` § QEMU |
| Install step 15 passes with an active interface and lease; DST options and hot spare built | Met: install step 15 (WSL) with a lease after the first IPL and again after an IPL from the disk alone (the cause was `QSTRUPPGM` stored as `QSYS/QSTRUP`); every DST option a panel or a reason; hot spares for mirror and parity sets, install step 16 |
| CI workflow present and green on the host-side jobs | `.github/workflows/ci.yml` (rustfmt, clippy `-D warnings`, deny/notices/notes/fmtcheck, host tests, fuzz 60 s, the three QEMU suites), `qemu.yml`, `release.yml`, `release-check.yml`; every job on GitHub-hosted runners, none self-hosted; the last main run's state is in the final report |
| `NOTICES.txt`, `QS400/SRC/` kernel source offer, `SOURCE-OFFER.txt` on the media | Met: `build notices` (289 crates, ten license kinds; fails on one outside `deny.toml`), `/QQS/ProdData/Notices/NOTICES.txt` on a system (`DSPSFWRSC` F19, `CPI9B83`), `QS400/SRC/` with the URL, SHA-256, configuration, build instructions and GPL-2.0, `SOURCE-OFFER.txt` naming cwellman@quartz.systems; QEMU 59 |
| UKI signed with the development Secure Boot key; `qs400-db.cer` and enrolment path shipped; `QSHWQUAL` and `INSTALL.md` say the right thing; two `build release` runs reproducible | Met: `tools/pesign` (Authenticode, RSA-2048); OVMF Secure Boot: unsigned refused, signed boots once `qs400-db.cer` is in `db`; `INSTALL.md` §6 enrolment steps; `QSHWQUAL` warns as the handoff words it; two WSL `build release` runs gave ISO `8580c9c3...e8bd6` both times; the release keys are secrets of the `release` environment (`v*` tags only) |
| Storage thresholds and `QSTGLOWACN` fire under QEMU; `*TIMZON` commands and tzdata-derived rules; `SAVSAVFDTA`, `WRKJRNRCV`, `RCVJRNE`, `WRKFCNUSG`, `TELNET`, `CHGSECA`/`DSPSECA` present; `QIBM_QSY_*` exit points called; euro CCSIDs added; power system values honoured or listed | Met: QEMU 140 (threshold, lower limit, every action, the exit point), 141 (UPS), 142 (batch restricted state), 143 to 145 (time zones from tzdata 2026f, CCSIDs against the primary's conversion), 146 to 148 (exit points, function usage, journal and save file commands); `QPWRRSTIPL` listed as a firmware setting |
| Release notes complete per §A5 and §E; `CLAUDE.md` and `ARCHITECTURE.md` current | Met: `RELEASE_NOTES_V1R1M0.md` rewritten (SQL at the 7.5 level with §10 imported, both format changes and their migration, every `BLOCKED(rel)` in the generated inventory, the §E list, the defined-but-inert system values, the textaudit count matching `baseline.txt` at 3,731); `CLAUDE.md`, `ARCHITECTURE.md` §10 |
| Marker inventory: only `BLOCKED(rel)` and `NEEDS-CODY`, every one listed in the notes | Met: 159 `BLOCKED(rel)`, 23 `NEEDS-CODY`; `kcbuild notes` fails on any other form and on a stale inventory |

Beyond the handoff, in the same round: QS/400 Access for Web (QS00AFW,
`ACCESSWEB.md`), every area golden-clean and live, QEMU 153 to 157 (153 to
156 together on main, run 42343131336; 157 with 153, run 42345432339);
program calls need `*OBJOPR` and `*EXECUTE`; the SST sign-on and disk
menus match the primary.

### NEEDS-CODY

| # | Item |
|---|---|
| 1 | ACS checklists: the host servers (`PHASE10.md`) and Run SQL Scripts (`SQL.md`) |
| 2 | `QSHWQUAL` on the Dell with the PERC H330 and the hypervisors (`QUALIFIED_HARDWARE.md`); booting the ISO on real firmware with Secure Boot and the QS/400 key enrolled |
| 3 | Physical tape acceptance (`MEDIA.md`) |
| 4 | Oracle changes on the primary: a `QAUDJRN` for the `DSPAUDJRNE` golden, a test profile for the special-authority IDs, a printer device, the JTOpen binary-read probe, a library for CREATE/DROP SCHEMA captures; delete `QSRELDEV1`, the device description a telnet capture created |
| 5 | The SST panels the oracle's service tools user cannot reach, and all of DST (no console access to the oracle) |
| 6 | The two `QSSECCHK` judgement calls (NEEDS-CODY 11 above) |
| 7 | The cut: `ci/version.sh prepare 1.1.0`, commit, `git tag v1.1.0`, push the tag (`releasing.md`) |
