# Security: open questions

Readings of IBM i behavior that Phase 6 had to choose without a
machine to ask. On 2026-10-22 the oracle kit ran on an IBM i 7.1
(`tools/conformance/oracle/sec/expected/`); the entries it settled are
gone from here and into the code (`cargo test -p conformance --test
sec_oracle` and `cargo test -p sec --test shipped_csv` compare with the
captures). What remains names what QS/400 does now and what would
settle it.

1. **Shipped profiles beyond the outfile's first 1000 bytes.** The
   `DSPUSRPRF OUTPUT(*OUTFILE)` capture was cut at 1000 bytes a record,
   so the attributes after `UPLANG` (country, CCSID, sort sequence,
   auditing, group authority type, supplemental groups, UID and GID,
   locale, home directory) are this system's defaults. Oracle: the
   outfile again with a longer record.

2. **Adopted authority and the owner's groups.** Settled: an adopting
   program does not reach the owner's group authorities (the machine
   refused). Kept here only because the capture covers `*CHANGE` to a
   file; other authorities are read the same way.

3. **Owner authority that falls short.** Settled: the owner entry
   decides, groups are not consulted (the machine refused).

4. **Password rule messages beyond `QPWDMINLEN`.** The machine sends
   `CPF22C2` for a password breaking `QPWDMINLEN`; every other
   composition rule now uses the same identifier with IBM's reason
   code in the text. Whether the machine distinguishes rules by
   identifier (`CPF2353` and its kin for the classic `QPWD*` values)
   is unsettled. Oracle: `CHGPWD` through `QSYCHGPW` with each rule
   broken, the identifiers in the error code.

5. **Missing special authority.** Settled for `CRTUSRPRF`: `CPF2292`,
   now used by `CRTUSRPRF` and `CHGUSRPRF`. Other commands that need a
   special authority the user lacks still report `CPF2222`. Oracle:
   `STRSBS` without `*JOBCTL`, `CHGSYSVAL` without `*ALLOBJ`.

6. **Owner and public authority of a new profile.** Settled: the
   creator (`QSECOFR`) owns a new profile.

7. **`PTYLMT` and `RUNPTY`.** Settled: `CHGJOB RUNPTY(10)` under
   `PTYLMT(5)` is refused with `CPF1337`; QS/400 refuses `JOBPTY` and
   `OUTPTY` below the limit and `RUNPTY` below ten times it.

8. **Level 20 to 30.** At the first IPL at 30 or above after 20,
   `*ALLOBJ` is taken from every profile whose class is not `*SECOFR`
   (and `*SAVSYS` stays). Oracle: a profile created at level 20 with
   `SPCAUT(*USRCLS)`, `DSPUSRPRF` before and after the change and IPL
   (two IPLs; not scripted).

9. **Required authorities.** Settled for `ADDPFM`: `*OBJALTER` alone
   suffices, so `required.toml` takes either of `*OBJMGT` and
   `*OBJALTER` for members, `CHGPF`/`CHGLF` and journaling. The rest
   of the table follows IBM's published tables as remembered. Oracle:
   `CHKOBJ` and each command as a user given exactly the authorities
   the table lists, then each minus one.

10. **API receivers.** `USRI0300` is now laid out as the machine's
    dump shows (`SYSAPI.md` §10), with two fields read from the dump
    rather than named (`00` at 632, `1` at 671). `OBJA0200`,
    `RTUA0100`'s entry and feedback and `AUTU0100` are still in IBM's
    field order with offsets of our own, and `QSYRUPWD` data is not
    IBM's. Oracle: call each API on IBM i with the same profiles and
    objects and compare the receivers byte for byte.

11. **`QSYCHGPR`.** It exists on the machine (`CHKOBJ QSYS/QSYCHGPR
    *PGM` succeeds); its parameters are unknown, so it is not
    provided. Oracle: `DSPPGM QSYS/QSYCHGPR` and the API finder for its
    parameter list.

12. **Authority command messages.** Settled: `DLTUSRPRF` of an owner
    is `CPF2215`, `ADDAUTLE` of a user on the list `CPF2282`,
    `RMVAUTLE` of one not on it `CPF2285`, `GRTOBJAUT` beyond one's
    own `CPF2227`. Still recollection: `DLTUSRPRF` of a primary group
    (`CPF2204` here), `RVKOBJAUT` (`CPF2253`, `CPF2254`). Oracle: run
    each refusal on IBM i and record the job log.
