# Changelog

One section per release, newest first, headed with its version (the one in
`VERSION`, `MAJOR.MINOR.PATCH`, which is the release `VxRyMz`) and date; a
release's section is its GitHub release's notes. Work not yet released goes
under Unreleased, which `ci/version.sh prepare X.Y.Z` turns into X.Y.Z's
section (see `docs/releasing.md`). The full notes of a release are
`docs/RELEASE_NOTES_VxRyMz.md`, on its install media too.

## Unreleased

### QS/400 V1R1M0

The first release of QS/400 meant for use outside development: the system
Phases 0 to 12 built, the Db2 for i SQL dialect at the 7.5 level, and the
on-disk and on-wire formats frozen. The full notes are
`docs/RELEASE_NOTES_V1R1M0.md`, also at the root of the install media.

- Upgrades in place from V1R0M0 (slip install): user libraries, the IFS,
  journals, spooled files and SQL tables kept; V1R0M0 saves restore.
- Install media for UEFI systems (CD, USB, BMC virtual media, PXE), with a
  response file for unattended and network installs.
- The UKIs are Secure Boot signed with the QS/400 key; enrol
  `QS400/EFI/keys/qs400-db.cer` or turn Secure Boot off (`INSTALL.md` §6).
- `NOTICES.txt` lists every third-party component and its license; the
  Linux kernel's source offer is `SOURCE-OFFER.txt` and `QS400/SRC/`.

## 1.0.0

V1R0M0, the development release. Not published.
