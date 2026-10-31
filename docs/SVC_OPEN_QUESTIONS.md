# Phase 11 open questions

Questions Phase 11 answered provisionally or settled by design, with
what QS/400 does now and how to settle each.

## Licensed programs

1. **License key form (resolved).** IBM's license key is 18
   characters, entered as `LICKEY` with three 6-character elements. It
   is a vendor-secret code, not a signature: nothing on the system can
   tell a genuine key from a forged one without IBM's algorithm.
   QS/400 signs keys with ed25519 so that the system checks them
   against a public key and nobody holding the system can mint them;
   a signature is 64 bytes and cannot fit 18 characters. Two forms were
   weighed: a short key carrying a truncated MAC with a signed key file
   beside it, and the whole signature typed by hand. A MAC needs a
   secret on the system, which defeats the point, and a truncated
   signature verifies nothing. **Chosen**: the key is the full
   signature plus a 28-bit typing check, 108 characters in a 32-letter
   alphabet without `I`, `L`, `O` and `U`, printed as six lines of IBM's
   three-group layout and entered as `LICKEY` with 18 elements; the
   fields the key binds (product, term, feature, serial number,
   processor group, usage limit, expiration date, vendor data) stay
   ADDLICKEY's own parameters. Keys are normally delivered as a key
   file for `LICKEYINP(*LICKEYFILE)`; the 108 characters are for
   keying by hand when the file cannot be moved (`docs/LICPGM.md` §7).
2. **QSZRTVPR offsets.** The "offset to additional information" value,
   PRDR0200's next-record offset base, PRDR0500's option record
   length, PRDR0700's release list and PRDR0800's entry layout are
   this implementation's reading of IBM's field lists
   (`docs/LICPGM.md` §4). Settle by calling QSZRTVPR on pub400 for
   `5770SS1 *BASE` and `5770SS1` option 33 in each format and dumping
   the receivers.
3. **Load state symbols for in-progress states.** States 35, 38, 50
   and 53 are given `*PACKAGED`/`*LOADED` and `*ERROR` here. Confirm
   against IBM's QSZRTVPR documentation of the symbolic state.
4. **Threshold notice.** The message sent when license use reaches
   the threshold is assumed to be `CPI9E19`; confirm in `QCPFMSG`.
5. **QSZCRTPL directory and agreement lists.** The optional groups'
   entry formats (`SDIR0100` and the agreement document list) are not
   read; a nonzero count is refused. Read IBM's QSZCRTPL page and add
   them.
6. **Target releases.** QSZCHKTG knows only Quartz releases, so IBM
   release names (`TGTRLS(V7R4M0)`) are refused. Whether QS/400 should
   map IBM target releases onto its own is a compiler question for the
   CRT* commands.

## Save and restore

The V7R1M0 oracle settled 27 restore rules
(`tools/conformance/oracle/save/primary/README.md`); these are left.

7. **`QALWOBJRST` and `QVFYOBJRST` refusals.** The messages a restore
   sends when `QALWOBJRST` refuses a system-state or adopting program,
   and when a signature is missing or does not verify, are `CPF3846` and
   `CPF3847` here. The oracle could not show them without changing the
   system values.
8. **An owner that does not exist.** A restored object whose saved owner
   is not on the system goes to `QDFTOWN` with `CPI3722`; the oracle has
   no profile it may delete to show IBM's message.
9. **`*USE` on a save file.** Restoring from a save file needs `*USE` on
   it here (`savf.read`); every oracle job ran as `QSECOFR`.
10. **Several libraries with failures.** `SAVLIB LIB(A B)` with objects
    not saved ends with `CPF3777` here; the oracle saved one library.
11. **`*AUTL` differences.** An authorization list that differs is
    refused under `ALWOBJDIF(*NONE)` with the owner's message
    (`CPF3766`) and kept under `*AUTL`; IBM's message for the refusal
    is not captured.
12. **Damaged saves.** An object whose records do not verify is not
    restored with `CPF3782`, the message IBM sends for a file that is
    not a save file; IBM's message for a damaged save is not captured.

## PTFs

13. **PTF message IDs.** `CPC3604` (loaded), `CPC3606` (applied),
    `CPC3608` (removed), `CPF3612` (requisite missing), `CPF3613`
    (superseded), `CPF3618` (needed by another), `CPF3606` (not loaded),
    `CPF3601` (already in that state), `CPF3586` (not signed), `CPF3598`
    (wrong product) are chosen, not captured: pub400 refuses every PTF
    command's prompt to an ordinary profile and the primary may not run
    them.
## Media

14. **Tape message identifiers.** The tape crate's mapping
   (`TapeError::message_id`, `docs/MEDIA.md`) against what is known of
   IBM's: `CPF6760` device not ready is the oracle's own answer
   (`34-devices.txt`, "Device TAP01 not ready") and stays. Fairly sure:
   `CPF6720` incorrect volume mounted, `CPF6768` volume write
   protected, `CPA4036` active file found (an inquiry with C and I),
   `CPF6772` volume cannot be processed. Unsure, kept until the
   oracle's `QCPFMSG` can be read (`DSPMSGD` over JDBC with `QCMDEXC`
   and `OUTPUT(*PRINT)`): `CPF6718` (used for the end of the volume; it
   may be IBM's "cannot allocate device"), `CPF6790` (file not found;
   IBM may use a CPF67xx neighbour), `CPF6708` (command ended because of
   an error), `CPF6745` (changer failure and cartridge not in the
   library), `CPF67D2`, `CPF6747`, `CPF6749` (library category, cartridge
   in use, no drive), `CPF6721` (device not available, used for a device
   not varied on).
15. **Operator inquiries.** The next-volume inquiry is `CPA4075` with
   replies C and G (unsure; IBM's tape volume request may be a CPA40xx
   or CPA67xx neighbour); the optical file-exists inquiry is `OPT1260`
   (Quartz's choice in the OPT range). Nobody answers an inquiry at
   once in the service, so each takes its default reply (cancel) and is
   kept answered on `QSYSOPR`; a job that waits for the operator's reply
   comes with an interactive reply path.
16. **Completion messages.** Quartz's choices, to check against the
   oracle: `CPC2618`/`CPC2619` device description made and changed,
   `CPF2605` device description exists, `CPF2702` device description not
   found (fairly sure of this one), `CPF2653` device in use,
   `CPC6773`/`CPC6774`/`CPC6775` volume initialized, checked, files
   duplicated, `CPCBC01` to `CPCBC09` the image catalog completions
   (the `CPFBC` escapes are `imgclg`'s, also Quartz's).
17. **One file per save.** IBM's `SAVLIB` of several libraries writes a
   tape file per library (`LABEL` each library's name); QS/400 writes
   one stream holding them all, labeled after the first. `RSTLIB` of any
   one of them with `LABEL(*SAVLIB)` therefore finds it only when it was
   saved first. Settle by writing a file per library once the save
   engine can end a stream per library.
18. **Virtual tape `ENDOPT(*UNLOAD)`.** IBM's virtual tape device unloads
   the image and mounts the next loaded one; QS/400 treats it as
   `*REWIND` for virtual devices (the entry stays mounted).
19. **Real device types.** A real drive's type and model are Quartz's
   generic ones (`3580 004`, `632C 002`, `3573 001`) rather than what
   the drive reports in its inquiry data (IBM shows e.g. `5746 001`,
   `6337 003`). Settle with the physical tape acceptance in
   `docs/MEDIA.md`.
20. **Media library cartridges.** `WRKTAPCTG`, `ADDTAPCTG`,
   `RMVTAPCTG`, `CHGTAPCTG` and `DSPTAPCTG` are defined and refuse; a
   save to a `TAPMLBnn` device refuses (`CPF6745`). Wiring them means
   keeping the cartridges' categories per library (the
   `tape::device::MediaLibrary` model) and running jobs on the
   library's drive with `LibraryProvider`.
21. **CPYTOTAP and CPYFRMTAP.** Not defined: they need tape file
   (`*TAPF`) device files, which this release does not have.

## Cleanup and backup

22. **Cleanup and backup message IDs.** `CPC1E20` to `CPC1E23`,
    `CPF1E24`, `CPF1E25` (cleanup), `CPC1E30`, `CPC1E31`, `CPF1E32`,
    `CPF1E33` (backup), `CPC1E35` (`PRTSYSINF`) and `CPF1E36` (an option
    that ends the subsystems run from inside one) are chosen, not
    captured: pub400 lets an ordinary profile prompt `CHGCLNUP` and
    `CHGBCKUP` but run neither.
23. **`STRTIME('HH:MM:SS')`.** pub400's `CHGCLNUP` prompt shows the
    literal `'HH:MM:SS'` as the start time; whether that is how IBM's
    prompt override shows a cleanup with no time set, or the account's
    own value, is not known. QS/400 ships `22:00:00`.
24. **`JRNRCVSIZ` on the `F10` display.** IBM shows it above
    "Additional Parameters", not under it; our prompter puts every
    `PMTCTL(*PMTRQS)` parameter under the title. The test compares the
    labels of that display, not their order.
25. **`+ for more values`.** IBM's text starts four columns right of
    where our prompter puts it (column 28 against 24); the cleanup and
    backup prompt tests compare it by its words. A fix belongs in
    `command/cmd/src/prompt/session.rs` with every prompt golden
    regenerated.
26. **Cleanup job and schedule entry names.** `QEZCLNUP` (the schedule
    entry, the cleanup job) and `QEZBCKUP` (a submitted backup) follow
    IBM's `QEZ` prefix; IBM's own names are not captured.
