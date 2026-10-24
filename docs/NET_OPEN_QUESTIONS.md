# Phase 10 open questions

Questions Phase 10 answered provisionally, with what QS/400 does now
and how to settle each against the oracle.

## IFS

1. **`CPFA0xx` for errors without an obvious pair.** `QCPFMSG` has no
   `CPFA0xx` for `ENOTDIR`, `EBADF`, `EAGAIN`, `EDEADLK` and most
   socket values; QS/400 sends `CPFA0D4` (file system error, with the
   number) for them, `CPFA0B1` (access problem) for `EROFS` and
   `EROOBJ`, and `CPFA0A2` for `EINVAL`. Settle by provoking each
   condition with `WRKLNK`/`CPY`/`MOV` under `/home/QSTEST` on the
   primary oracle and reading the job log.
2. **Share-mode refusal errno.** An `open()` refused by another open's
   share mode is `EBUSY` here; IBM may answer `ETXTBSY` for some
   combinations. Probe with two JDBC sessions calling `QP0LOPEN`-style
   CL or with `CPY` while a file is held open by `EDTF`.
3. **Check-out refusals.** Writing, renaming or unlinking an object
   checked out by another user is `EBUSY` here. Confirm the errno and
   the `CPFA0xx` (`CPFA09E` assumed).
4. **Default umask.** A new owner's umask is 0o022. IBM documents the
   job's initial `umask` as 0; confirm with `umask` in QSH on pub400.
5. **Directory CCSID on `mkdir`.** A new directory takes its parent's
   CCSID; IBM may tag it with the job CCSID. Check `st_ccsid` of a
   directory made by `MKDIR` from a job with CCSID 37 and one with 273.
6. **Group authority after `CHGPGP` with no private authority.** QS/400
   gives the new group the authority the object already had for its
   group (the `rwx` triple), POSIX-style; IBM may give `*EXCLUDE`.
   Check `DSPAUT` after `CHGPGP` on a file created with mode 0o750.
7. **`CHGATR` authorities.** PC attributes, CCSID and text need the
   owner or `*W`; storage, save and scan attributes the owner or
   `*OBJALTER`. IBM's `CHGATR` documentation lists per-attribute needs
   that may differ (`*OBJMGT` for some).
8. **Source members as streams.** Reading a source member through the
   IFS gives its lines without sequence numbers, trailing blanks
   removed, each ended by a line feed in the member's CCSID. IBM may
   end lines with CR LF or keep trailing blanks in binary mode; compare
   `CPYTOSTMF` and a QSH `cat` of a member on the oracle.
9. **Fallback owner.** A node without metadata whose UID maps to no
   profile is owned by `QDFTOWN`. IBM's `RCLLNK` may assign `QDFTOWN`
   only for objects whose owner profile was deleted; nodes made outside
   the system have no IBM equivalent.
10. **`O_CREAT` on a dangling symbolic link.** QS/400 answers `EEXIST`
    (the link itself is the last name); POSIX creates the link's target.
    Check IBM with a link to a missing file under `/home/QSTEST`.
