# Changelog

One section per release, newest first, headed with its version (the one in
`VERSION`, `MAJOR.MINOR.PATCH`, which is the release `VxRyMz`) and date; a
release's section is its GitHub release's notes. Work not yet released goes
under Unreleased, which `ci/version.sh prepare X.Y.Z` turns into X.Y.Z's
section (see `docs/releasing.md`). The full notes of a release are
`docs/RELEASE_NOTES_VxRyMz.md`, on its install media too.

## Unreleased

- QSI: a standalone system answers base's `Resources` (`0x0009`, bit 28,
  `caps::RESOURCES`): its processors, processor used, main storage, the
  system ASP and its use, temporary storage, jobs and threads, as
  `WRKSYSSTS` counts them, so a management console shows them.

## 1.2.0 (2026-11-22)

### QS/400 V1R2M0

Three of the features V1R1M0 left out, NetServer, NFS and the Directory
Server, and every fix since V1R1M0. The full notes are
`docs/RELEASE_NOTES_V1R2M0.md`, also at the root of the install media.

- Upgrades in place from V1R1M0 (slip install). Each user changes their
  password once before using NetServer (the NT hash is made then).
- NetServer: SMB 2.0.2 to 3.1.1 file and print serving over the IFS for
  Windows, macOS and Linux clients, with signing, encryption, leases,
  durable handles, change notification and point and print; sign-on by
  NTLMv2 against user profiles; `QZLS*` APIs, Navigator and QAP.
- NFS: the server (versions 3, 4.0 and 4.1, with MOUNT, NLM and NSM) and
  the client (`MOUNT TYPE(*NFS)`), `EXPORTFS` and `/etc/exports`, file
  handles that survive an IPL, Navigator and QAP.
- Directory Server: an LDAPv3 server in the base OS (`STRTCPSVR
  SERVER(*DIRSRV)`) with ACLs, password policy, the user profiles
  projected as entries, TLS and SASL `EXTERNAL`, publishing, `QGLDCLNT`
  and the QSH `ldap*` tools.
- QSI: a standalone system answers a paired management console
  (`QSIDSRV`, `STRQSIPAIR`, `WRKQSICSL`): its state, power down, reference
  codes, problems, dumps and its console. In a partition of QS/VS the
  system has a service channel to the hypervisor.
- PTFs install the way IBM i users try first: `CRTSAVF`, an FTP `put` in
  binary into the save file, `LODPTF`, `APYPTF`. The FTP server takes a
  save stream of any length, `CPYFRMSTMF` fills a save file, and every
  refusal of a save file says why. Each PTF's notes say how to install it,
  and no PTF is published unless it installs on its release as shipped
  (the gate, `releasing.md`).
- After `RMVPTF` of a LIC PTF IPLed an older LIC, the PTF database was
  empty; its records are again as V1R1M0 reads them.
- Navigator's Fixes page has Check now and says when no check was made.
- NetServer: a Linux client's whole-file lock now holds against clients
  on other connections.
- Access for Web's 5250 display takes keys, clicks and typing; the three
  web products move between pages in place.
- Not in V1R2M0: an SNMP agent (the definitions only), and the rest the
  notes list under "Deliberately not in V1R2M0".

## 1.1.0 (2026-11-08)

### QS/400 V1R1M0

The first release of QS/400 meant for use outside development: the system
Phases 0 to 12 built, the Db2 for i SQL dialect at the 7.5 level, and the
on-disk and on-wire formats frozen. The full notes are
`docs/RELEASE_NOTES_V1R1M0.md`, also at the root of the install media.

- Upgrades in place from V1R0M0 (slip install): user libraries, the IFS,
  journals, spooled files and SQL tables kept; V1R0M0 saves restore.
- Install media for UEFI systems (CD, USB, BMC virtual media, PXE), with a
  response file for unattended and network installs.
- SQL at the Db2 for i 7.5 level: DDL, queries, SQL PL and triggers, types,
  embedded and dynamic SQL, the call level interface, DRDA, the `QSYS2`
  catalogs and services, `STRSQL`; what is not implemented is refused with
  IBM's SQLCODE.
- Licensed programs on the media: the operating system (QS00SS1), the
  application development tools (QS00WDS), the HTTP Server with its
  administration (QS00DG1), Navigator for QS/400 (QS00NAV) and QS/400 Access
  for Web (QS00AFW: a 5250 session, database, files, printer output,
  messages, jobs and commands in the browser).
- Hot spare disks, storage thresholds and the lower-limit actions, UPS power,
  time zones from tzdata, more CCSIDs, function usage, the `QIBM_QSY_*` and
  work-management exit points.
- QAP versions 8 and 9 (drafts) for Code for QS/400: the `Develop` service
  (library lists, objects, source members with sequence numbers and dates,
  `Compile` with the compiler's diagnostics), `Jobs`, the SQL routine and
  index lists, and `Debug` (breakpoints, stepping, variables, service entry
  points). The RPG, CL, DDS, SQL and command front-ends build as libraries
  (`front` feature, `analyze()`).
- Hardware resource names (`CMNnn` and the rest) keep their names across
  IPLs and hardware changes; a missing card stays listed as not detected.
- The UKIs are Secure Boot signed with the QS/400 key; enrol
  `QS400/EFI/keys/qs400-db.cer` or turn Secure Boot off (`INSTALL.md` §6).
- `NOTICES.txt` lists every third-party component and its license; the
  Linux kernel's source offer is `SOURCE-OFFER.txt` and `QS400/SRC/`.

## 1.0.0

V1R0M0, the development release. Not published.
