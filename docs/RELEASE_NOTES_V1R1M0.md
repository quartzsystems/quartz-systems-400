# QS/400 V1R1M0 release notes

V1R1M0 is the first release of QS/400 meant for use outside
development. It is the system Phases 0 to 11 built, with the gaps those
phases carried closed, the on-disk and on-wire formats frozen, and the
operational pieces a release needs added in Phase 12 (`PHASE12.md`).

## Upgrading

- A V1R0M0 system upgrades in place: boot the V1R1M0 media, choose
  Install Licensed Internal Code, option 1 (slip install). User
  libraries, the IFS, journals and spooled files are kept; every shipped
  object is replaced and its signature checked (`INSTALL.md` §4).
- A `SAVLIB` made on V1R0M0 restores on V1R1M0 (`RSTLIB`); the save
  format did not change.
- V1R0M0 media is refused over a V1R1M0 system (no downgrade).
- The formats frozen at V1R1M0 are listed in `tools/fmtcheck/formats.toml`
  and in each format document's "Frozen at V1R1M0" section. A later
  release changes one only with a version bump and a migration.

## What is new in V1R1M0

**Languages and compilers.** ILE RPG `LIKEFILE` on procedure
parameters, `DATA-INTO` and `DATA-GEN` with shipped JSON and XML parsers
and generators (and QS/400's own `QOAR` interface members), Open Access
handlers, `DUMP(A)` into `QPPGMDMP`. RPG III: the QS00RPG licensed
program with `CRTRPGPGM` and `CVTRPGSRC` (`RPG3.md`). CL `%OFFSET` as a
value and a `CHGVAR` target. PCML for `CRTBNDRPG`/`CRTRPGMOD`
`INFOSTMF`.

**Network.** A DHCPv4 client for `*IP4DHCP` interfaces; the install
response file over HTTP, HTTPS and TFTP, and beside a PXE boot file;
telnet automatic sign-on (`INIT0100`, RFC 4777 substitutes, `QRMTSIGN`);
spooled-file data from the print host server; SMTP relay credentials
(`ADDSMTPLE TYPE(*HOSTAUTH)`); the DDM/DRDA server on 446, DDM files and
record-level access to a remote file, server authentication entries;
`*WSCST` objects with IBM's tag language and the shipped objects; QAP
protocol version 7 (draft), the `Manage` service.

**Work management and performance.** `ENDSBS *ALL` ends every job but
the console's and reaches the restricted state; `SAVSYS` needs it.
Collection Services (`STRPFRCOL`, `ENDPFRCOL`, `CFGPFRCOL`,
`CHKPFRCOL`, `CRTPFRDTA`), `*MGTCOL` objects and the `QAPM*` files;
`WRKSYSACT` under QS00PT1; `QPFRADJ`; job accounting (`QACGLVL`,
`ACGCDE`, `CHGACGCDE`, `QACGJRN`).

**Help and national language support.** Help for every panel, display
file, command, parameter and message (`tools/helpaudit` at 100%); F1, F2
and F11 at the prompter; search indexes (`*SCHIDX`, `QHSS1`) and
`DSPHLPDOC`. CCSIDs kept on message files, panel groups and display
files; `INSLNG` and the secondary language libraries (`QSYS2924`
primary, a `QSYS2931` Spanish scaffold); dates, times and decimals in
the job's formats through one path.

**Hardware and virtualization.** More drivers in the kernel (Intel igc,
Broadcom tg3/bnxt, Realtek r8169, vmxnet3, virtio-scsi, Hyper-V, vsock,
TPM, USB UAS); `QSHWQUAL`, a hardware qualification report from the
install media or the system; a guest agent speaking the QEMU guest agent
protocol (Proxmox and libvirt see it), shown in `WRKHDWRSC *CMN`;
`QUALIFIED_HARDWARE.md`.

**Security and service.** `QSSECCHK`, a security check report;
`*PUBLIC *EXCLUDE` kept on the commands and system objects that ship
with it; problem reports sent to the update service
(`SNDSRVRQS ACTION(*REPORT)`).

**Web.** The HTTP Server (QS00DG1) with HTTP Server Administration on
port 2001, and Navigator for QS/400 (QS00NAV) on 2002 and 2005, ship as
licensed programs on the media (`HTTPADMIN.md`, `NAVIGATOR.md`); both were
checked again on the frozen V1R1M0 tree (QEMU steps 64, 70, 86 to 88).

**Tools.** `tools/fmtcheck` (the format freeze), `tools/fuzz` (a
coverage-guided fuzzer in Rust over 18 parsers), `tools/helpaudit`,
`tools/textaudit`, `tools/kcbuild` (the Knowledge Center, on the install
media), `build soak`, `build release`.

## Deliberately not in V1R1M0

- An SMTP server (the SMTP client sends mail).
- BIOS boot: the media and the system boot under UEFI only (the kernel
  is an EFI-stub UKI, `ARCHITECTURE.md` §5).
- Native GPU drivers: the console uses the firmware's framebuffer
  (`CONSOLE.md`).
- A diskless network install: the response file comes over the network,
  the payload from a CD, a USB stick or BMC virtual media.
- Access for Web (QS00AFW, reserved) and the Knowledge Center served over
  HTTP from `/QQS/ProdData/KnowledgeCenter`: after V1R1M0. The
  Knowledge Center is on the install media (`QS400/KC/`).
- BRMS, save while active, parallel saves, optical media libraries,
  remote journaling, IASPs (`PHASE11.md` Deferred).

## Known limits (`BLOCKED(phase12)`)

| Item | Where |
|---|---|
| 3,785 text literals on display paths still to move into message files and panel sources; the count can only go down (`tools/textaudit/baseline.txt`) | developer tools, service listings, job panels |
| Output fields that run past column 80 into the next row (`DSPSRVPGM` signature, `DSPMSGD` list) | `ui/uim` renderer |
| `WRKSYSSTS`, `WRKSHRPOOL`, `WRKDSKSTS` screens: IBM's are display files on a hand grid; ours are UIM panels with the same data | `ui/uim/pnlgrp/QGWC*` |
| Another job's call stack in `WRKJOB` | `work/qsjob/src/console/jobcmd.rs` |
| A `*USA` time field's record buffer holds `hh:mm:ss`; IBM i holds `hh:mm AM` | the database's buffer conversion |
| IFS directories in product save streams (`SAVLICPGM`/`RSTLICPGM`) | `licpgm/lpstore/src/savfile.rs` |
| The call ABI: a pointer to a by-reference parameter, and by-reference against `* value` through a procedure pointer | `ABI.md` §3; a module format bump |
| HTTPAPI over TLS: no ILE binding of `QSOSSLSR` (`gsk_*`); IBM's `QSYSINC` headers are not shipped | `PHASE9.md` |
| DDM files: CHAIN by record number, READP/READE and SETLL by key end with CPF5149; compilers do not fetch a remote record format | `DDM.md` |
| Open Access: the indicator and feedback areas are not passed; `DUMP(A)` has no file sections | `RPG.md` |
| The prompter shows long parameters on one row (IBM wraps them over several) | `command/cmd` prompter |

## Known differences from IBM i

The deviations each phase records stand (`PHASE1.md` to `PHASE12.md`,
the `*_OPEN_QUESTIONS.md` files). The main ones: the product ID of the
operating system is `QS00SS1` (with `5770SS1` as an alias); save files
are their own object type `*SAVF`; the help, message and report text is
Quartz's own (IBM's layouts, not its words); `INSLNG` is QS/400's front
end to `RSTLICPGM RSTOBJ(*LNG)`; the `QSYS2931` Spanish library is a
scaffold with machine-translated first-level text for 90 messages; the
install media's LIC menu has a fourth option, `QSHWQUAL`; Collection
Services messages `CPIB920`-`924` and `CPFB993`-`995` are QS/400's own
identifiers.

All RPG conformance was measured against pub400.com (IBM i V7R5M0): the
primary oracle (IBM i V7R1M0) has no RPG compiler.

## Before shipping (Cody)

The `NEEDS-CODY` list in `PHASE12.md`: sign the release with the real
key (`QS400_SIGNING_KEY`), the ACS checklist, the hypervisor and Dell
qualification runs, physical tape, the oracle commands that need a
configuration change, and the tag.
