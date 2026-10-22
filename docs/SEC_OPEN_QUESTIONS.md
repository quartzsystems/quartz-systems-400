# Security: open questions

Readings of IBM i behavior that Phase 6 had to choose without a
machine to ask. Each names what QS/400 does now and the oracle test
that settles it: a command or program run on a real system whose
output goes under `tools/conformance/oracle/sec/` when Cody supplies
it. The tests that read those outputs skip while they are absent.

1. **Shipped profiles' special authorities.** IBM's per-profile table
   was not at hand. `sec::profile::shipped::PROFILES` gives `QSYSOPR`
   and `QPGMR` `*JOBCTL *SAVSYS`, `QSRV` `*JOBCTL *SAVSYS *SERVICE`,
   `QTCP` `*JOBCTL *IOSYSCFG`, `QCOLSRV`, `QPEX` and `QSNADS` `*JOBCTL`,
   `QLPAUTO` and `QLPINSTALL` `*ALLOBJ *JOBCTL *SAVSYS *SECADM`, the rest
   none. Oracle: `DSPUSRPRF USRPRF(x) OUTPUT(*OUTFILE)` for every
   shipped profile, compared field by field.

2. **Adopted authority and the owner's groups.** The Phase 6 brief says
   to check an adopting owner "with steps 1-5"; IBM's documentation as
   remembered says adopted authority is the owner's own and does not
   include its group profiles. QS/400 follows IBM: the owner's
   `*ALLOBJ`, ownership, private and list entries. Oracle: an
   `USRPRF(*OWNER)` program owned by `OWNA` (member of `GRPA`, which has
   `*CHANGE` to `LIB/F`; `OWNA` has none) opens `F` for update; record
   whether the open succeeds.

3. **Owner authority that falls short.** When the user owns the object
   and the owner's authority was reduced below what is needed, QS/400
   does not go on to groups or `*PUBLIC` (the owner entry decides) but
   does add adopted authority. Oracle: `GRTOBJAUT ... USER(*OWNER)`
   reduce, then `CHKOBJ` as the owner with a group that has `*ALL`.

4. **Classic password rule messages.** Composition failures use
   `CPF4AA7` with IBM's reason codes for every rule, including the
   `QPWD*` system values; IBM's older per-rule `CPF23xx` messages for
   those values are not reproduced. `QPWDLMTCHR` uses reason 24, which
   is Quartz's. History, change block, validation program and current
   password failures use `CPF22C3`, `CPF22C5`, `CPF22C6` and `CPF2362`.
   Oracle: `CHGPWD` through `QSYCHGPW` with each rule broken, the
   message identifiers in the error code.

5. **Missing special authority.** Commands that need a special
   authority the user lacks report `CPF2222`. Oracle: `CRTUSRPRF` by a
   user without `*SECADM`, the escape message.

6. **Owner and public authority of a new profile.** `CRTUSRPRF` makes
   the creator the owner of the `*USRPRF` and gives the profile no
   private authority to itself; `*PUBLIC` is `AUT` (default
   `*EXCLUDE`). Oracle: `DSPOBJAUT OBJ(NEWUSR) OBJTYPE(*USRPRF)` after
   `CRTUSRPRF` by a security officer.

7. **`PTYLMT` and `RUNPTY`.** `PTYLMT` limits `JOBPTY` and `OUTPTY`;
   QS/400 also keeps `RUNPTY` from being numerically below it, as the
   brief asks. Oracle: `CHGJOB RUNPTY(1)` as a user with `PTYLMT(5)`,
   then `RTVJOBA`.

8. **Level 20 to 30.** At the first IPL at 30 or above after 20,
   `*ALLOBJ` is taken from every profile whose class is not `*SECOFR`
   (and `*SAVSYS` stays). Oracle: a profile created at level 20 with
   `SPCAUT(*USRCLS)`, `DSPUSRPRF` before and after the change and IPL.

9. **Required authorities.** `security/sec/required.toml` follows IBM's
   published tables where remembered; where IBM accepts one of two
   authorities (`*OBJMGT` or `*OBJALTER` for `ADDPFM`, `CHGPF`,
   `STRJRNPF`), QS/400 requires the one named first. Oracle: the
   authority matrix kit (`tools/conformance/oracle/sec/`), `CHKOBJ` and
   each command as a user given exactly the authorities the table
   lists, then each minus one.

10. **API receivers.** `USRI0300` past the supplemental groups,
    `OBJA0200`, `RTUA0100`'s entry and feedback and `AUTU0100` are
    laid out in IBM's field order with offsets of our own
    (`SYSAPI.md` §10). `QSYRUPWD` data is not IBM's. Oracle: call each
    API on IBM i with the same profiles and objects and compare the
    receivers byte for byte, outside the password data.

11. **`QSYCHGPR`.** No IBM API by that name could be identified; it
    is not provided. Oracle: the IBM i API finder.
