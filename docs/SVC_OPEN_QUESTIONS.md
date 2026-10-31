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
   (Quartz's choice in the OPT range). Replies are real now
   (`docs/MEDIA.md`, "Operator replies"): a reply the operator already
   sent to the same inquiry, else the job's `INQMSGRPY` (`*DFT`, the
   reply list for `*SYSRPYL`); with `*RQD` the inquiry waits on
   `QSYSOPR` and the operation ends until it is run again after the
   reply, since the service cannot wait inside a command. Still open:
   whether IBM keeps a save waiting in `MSGW` for an interactive job's
   operator (it does for batch); settle by a job-side reply path when
   the service can suspend a command.
16. **Completion messages.** Quartz's choices, to check against the
   oracle: `CPC2618`/`CPC2619` device description made and changed,
   `CPF2605` device description exists, `CPF2702` device description not
   found (fairly sure of this one), `CPF2653` device in use,
   `CPC6773`/`CPC6774`/`CPC6775` volume initialized, checked, files
   duplicated, `CPC6776`/`CPC6777` cartridge category set and ejected,
   `CPC6778`/`CPC6779` records copied to and from tape, `CPC7301`/
   `CPC7302` tape file made and changed, `CPCBC01` to `CPCBC09` the image
   catalog completions (the `CPFBC` escapes are `imgclg`'s, also
   Quartz's).
17. **One file per save.** Resolved: a `SAVLIB` of several libraries to
   tape or optical writes a file per library, labeled with its name
   (as IBM does), and `RSTLIB` of several libraries reads each one's
   file by its label. A save file still holds one stream.
18. **Virtual tape `ENDOPT(*UNLOAD)`.** Resolved: the entry in use goes
   back to `*LOADED` and the next loaded entry is mounted, as IBM's
   virtual tape does. Open: what IBM mounts when no other entry is
   loaded; QS/400 keeps the entry mounted (the catalog always mounts a
   loaded entry).
19. **Real device types.** A real drive's type and model are Quartz's
   generic ones (`3580 004`, `632C 002`, `3573 001`) rather than what
   the drive reports in its inquiry data (IBM shows e.g. `5746 001`,
   `6337 003`). Settle with the physical tape acceptance in
   `docs/MEDIA.md`.
20. **Media library cartridges.** Resolved for virtual libraries
   (`CRTDEVMLB RSRCNAME(*VRT)` over a `*TAP` catalog): `WRKTAPCTG`,
   `DSPTAPCTG`, `ADDTAPCTG`, `CHGTAPCTG`, `RMVTAPCTG`, `WRKMLBSTS` and
   saves and restores mounting cartridges through the library. IBM i
   has no virtual tape library; this one is Quartz's, so its drive
   names (`<rsrc>D1`, `<rsrc>D2`), its slot count and the `Inserted`/
   `Available`/`Mounted`/`Ejected` statuses are Quartz's choices. Open:
   a real library's changer (`SgChanger`) behind the same commands,
   settled with the physical tape acceptance.
21. **CPYTOTAP and CPYFRMTAP.** Resolved: both, over `*FILE TAPF` tape
   files (`CRTTAPF`, `CHGTAPF`). Open: `OVRTAPF`; IBM's message
   identifiers for the copies; `CPYFRMTAP OUTFMT` shown on the first
   page of the prompt from the start (pub400 shows it once the prompt
   has been entered once; the prompter has no rule for that).

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

## Install and recovery

27. **`SAVSYS`'s completion message.** `CPC3703` here, with the device
   in its data; IBM's message for a completed system save was not
   captured (the oracles may not run `SAVSYS`).
28. **`SAVSYS` while active.** IBM i ends `SAVSYS` unless the system is
   in restricted state; QS/400 saves while active (product loads and
   security data are consistent units in the store). Whether to refuse
   as IBM does is open until `ENDSBS *ALL` leaves a usable console.
29. **A blank virtual tape.** Resolved: `SAVSYS` requires a labeled
   volume as IBM does and refuses a blank or unlabeled one with
   `CPF6772` before it builds anything; `INZTAP` labels it. A catalog's
   `ADDIMGCLGE FROMFILE(*NEW)` volume is labeled with its `VOLNAM` when
   it is made, so a new virtual tape needs no `INZTAP`.
30. **Install message identifiers.** The install SRCs C6004060, C6004070,
   C6004130 to C6004160 and A6004080 are Quartz's in IBM's C600 range;
   IBM's install codes are not documented to that level.
31. **The response file's agreement.** Listing a product under `licpgm`
   accepts its software agreement as QSECOFR; IBM's automatic install
   asks for the agreements on the console. Kept for unattended installs.

## Object integrity

32. **What `CHKOBJITG` checks.** Every shipped object is signed, but the
    signatures of `*PGM`, `*SRVPGM`, `*MODULE` and `*CMD` only are
    checked, the types IBM i signs: the others (job queues, devices,
    data areas) change in use, so a signature over their content stops
    matching on a healthy system (the first install run found `QSPL
    *JOBQ` and `QSCONSOLE *DEVD`). Damage is checked for every type. The
    completion `CPC39E0` and escape `CPF39E1` are Quartz's choices; IBM's
    `CHKOBJITG` writes an outfile instead and its messages were not
    captured. Open: an `OUTFILE` in IBM's layout, once captured.
33. **What the recovery's `RST` leaves.** In the install suite's step 9
    the recovery's `RST OBJ(('/*') ('/QSYS.LIB' *OMIT))` restores 12
    links and leaves 7 (`CPF3838`), and `RSTLIB SAVLIB(*NONSYS)` ends
    `CPF3770` for a library with nothing to restore; the user library,
    the profile and `/rcvdata` come back. The 7 are most likely
    directories the fresh install already made; the service now logs
    each one not restored to serial, which settles it on the next run
    that keeps its log.
