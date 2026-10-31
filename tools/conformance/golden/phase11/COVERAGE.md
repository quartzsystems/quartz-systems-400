# Phase 11 golden coverage

Every pub400 (V7R5M0) capture under this folder, and what compares
it with QS/400. The captures hold IBM's text; nothing here is copied
into shipped files. What is compared is the interface: geometry,
keywords, order, field lengths, defaults, option numbers, keys.

## How the comparisons work

- **Prompts** (`command/sys-cpps/tests/phase11_prompts.rs`). pub400
  shows keywords (fields from column 50); our default view does not
  (fields from column 37), so columns are not compared. Each prompt
  and its following pages (`-2`, `-3`, ...) become a list of
  parameters read from the `.txt` and `.attr` grids: the keyword in
  column 35, the input field (underlined, displayed attribute run)
  with its length and value, the fields of the qualifier and element
  rows under it, and the `Additional Parameters` divider. Ours comes
  from the prompter's screens, paged with Page Down to `Bottom` (or as
  many pages as the capture has). The test compares the keyword order
  with the divider, each field's length and default, the parts, the
  title's `(CMD)` and the function keys of the first page. Fields of
  40 or more positions count as long on both sides (the prompter cuts
  at 44 less the indent). Each case lists the parameters pub400 has
  and we do not (`ibm_only`), the ones we show and pub400 does not
  (`ours_only`), and the ones whose field differs for a reason below
  (`known`); the test checks those lists stay true.
- **Panels** (`ui/uim/tests/phase11_panels.rs`). Rendered with the
  capture's visible values, compared by geometry: input fields (row,
  column, length from the `.attr` grid), the column starts of the
  heading rows, the `n=` options and the function keys with their
  places, the `More...`/`Bottom` place. The parts that differ are
  recorded per panel; the test fails when the set changes.
- **Task menus** (`ui/menus/tests/task_menus.rs`): rendered and
  compared row by row, as for Phase 8.
- **Command menus** (`ui/menus/tests/generated.rs`): option numbers
  and order of the commands both sides list, the commands only IBM
  lists, and how many leading options number exactly as IBM's.
- **Refused captures**: pub400's ordinary profile was refused (the
  shipped `*EXCLUDE` public authority). `refused_commands_are_defined`
  checks each such command is defined; the special authority our
  processing program asks for is noted in its row.

## Fixed while comparing

The prompter (`command/cmd/src/prompt/`):

- A field is as wide as its longest special, single or listed value
  (IBM's rule); a restricted number is as wide as its longest value;
  `INLPMTLEN(n)` gives the width. Fields had cut `*MOUNTED` to
  `*MOUNT`, `*LIBASP` to `*LIBA`, `*JOBD` to `*JOB`, `*NONE` to `*NON`.
- A `PMTCTL` condition over a parameter with no value is undecided:
  hidden on the first display, shown after `F10` (pub400's `SAVLIB`,
  `SAVOBJ`, `SAVSECDTA`, `SAVCFG` show only `LIB`/`DEV` first).
- A qualified parameter whose default is a single value shows its
  other qualifiers blank (`JOBD(*USRPRF)`, `MSGQ(*SYSOPR)`), as
  pub400's Phase 8 captures of `SBMJOB` do.
- The key parameters' display offers `F10` when the command has
  conditional or additional parameters, and `F10` there shows them.

The definitions (`command/sys-cpps/cmds/`): `SAVLIB`, `SAVOBJ`,
`SAVCHGOBJ`, `SAV`, `SAVSECDTA`, `SAVCFG`, `CRTSAVF`, `CHGSAVF` in
pub400's order with its `PMTRQS` split (`SAVACT`, `SAVACTWAIT`,
`SAVACTMSGQ`, `ASPDEV` are additional; `OBJJRN` is not); the media
parameters of `SAV` and `SAVLIB`'s `STRLIB` conditional on the device;
`VOL` 32 long; `OBJTYPE` 7; `LABEL(*LIB)` for `SAVOBJ`/`SAVCHGOBJ`;
`SAVCHGOBJ REFDATE(*SAVLIB)` no longer mapped to a date the analyzer
refused (`?SAVCHGOBJ` failed with CPD0082); `SAVSECDTA ASPDEV(*ALLAVL)`;
`CRTSAVF ASP(*LIBASP)`; `WAITFILE` a 2-byte integer; `DSPSAVF OUTPUT`;
`DSPTAP`/`DSPOPT DATA` 7, `DUPTAP FILES` 7, `CRTDEVTAP`/`OPT`/`MLB`
`TYPE` and `MODEL` 10.

The menus: `GO RESTORE` page 3 headings; `GO TAPE` (linked from
`GO DEVICE`, missing until now).

## Recorded differences

- Save commands: pub400 has `SYNCID`, `PVTAUT` (`SAVLIB`), `STG`,
  `SELECT`, `CMDUSRSPC`, `SORT`, `PRECHK` (`SAVSECDTA`, `SAVCFG`),
  `EXPDATE` and `USEOPTBLK` (`SAV`), `STRLIB` (`SAVCHGOBJ`); we have
  `LABEL` and `MEDDFN` on `SAVSECDTA`/`SAVCFG`, `FILEMBR` on
  `SAVCHGOBJ`, and `SAV`'s additional parameters, which pub400 shows
  only on conditions it alone knows. `SAVACTMSGQ`'s library shows
  `*LIBL` on pub400 beside `*NONE`; pub400 fills the element defaults
  of `OMITOBJ` and `FILEMBR` after Enter; `SAVCHGOBJ OBJ` shows no
  default there; `SAV PATTERN` shows `'*'` there.
- `CHGSAVF` opens on its key parameter on pub400.
- Integer fields sized by definitions we cannot see: `DUPOPT
  ALWMEDERR`, `INZOPT CCSID`, `CRTDEVMLB` waits and `RSCALCPTY`;
  `INZOPT TYPE` (10), `INZTAP NEWOWNID` (17, the label holds 14),
  `ADDIMGCLGE REPLACE`, `VFYIMGCLG LNG`, `CRTDEVTAP SWTSET`,
  `LCLINTNETA`, `ROBOTHOST`. `DSPTAP`/`DSPOPT` do not offer
  `OUTPUT(*OUTFILE)`.
- `CRTIMGCLG REFIMGCLG` shows before F10 here; the new-image
  parameters of `ADDIMGCLGE` and the device descriptions' `ONLINE`,
  `MSGQ`, `TEXT`, `AUT` (`CRTDEVOPT`) show after F10 here, not there.
- `WRKSFWAGR` lists release and status where pub400 lists the option;
  pub400 shows the selected language, `6=Print`, F11, F13, F19, F22,
  its keys a row higher and a one-position `Opt` field (ours is two).
- `DSPSAVF` shows the save file and count above the list and a text
  column where pub400 shows owner, size in K and data saved; it lacks
  `5=Display`, F11 and F16; the `Opt` field is two positions.
- `DSPSFWRSC`'s F11 view: the type and release columns sit one column
  off, the release shows six characters, keys and `Bottom` a row lower.
- The prompter's own geometry, unchanged: `More...`/`Bottom` on row 22
  and the keys on rows 23 and 24 (pub400: 21, 22 and 23); the `+ for
  more values` field is one position (pub400: a value field); F11
  puts the keyword in the choices column (pub400: column 35, the field
  at 50).
- Command menus: IBM's menus end with links to related menus and
  list commands the name rule does not place (`generated.rs` lists
  them); `CMDTRC` and `CMDSRV` are curated on IBM's side.
- Commands without a special-authority check where IBM refuses an
  ordinary user: the problem log, dumps, traces, `WRKCNTINF`,
  `RCLDBXREF`, `DSPPTF`, `DSPPTFCVR`, `WRKPTFGRP`, `WRKLICINF`; the
  restores check object authority only. `GO LICPGM` and `GO SERVICE`
  are not refused here.

## Counts

| Status | Goldens |
|---|---|
| compared by a test | 112 |
| covered (a later page of a compared list) | 175 |
| refused on pub400: command or menu checked | 45 |
| absent on pub400 | 3 |
| deferred or not compared | 21 |
| pending (another agent) | 13 |
| total | 369 |

## Every golden

| Golden | Status |
|---|---|
| `prompts/prompt-addimgclge` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-addimgclge-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-addlickey-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-anzprb-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-apyptf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-chgbckup` | pending (agent): cleanup/backup commands |
| `prompts/prompt-chgbckup-f10` | pending (agent): cleanup/backup commands |
| `prompts/prompt-chgbckup-f10-2` | pending (agent): cleanup/backup commands |
| `prompts/prompt-chgclnup` | pending (agent): cleanup/backup commands |
| `prompts/prompt-chgclnup-f10` | pending (agent): cleanup/backup commands |
| `prompts/prompt-chgipla-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-chglicinf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-chgprb-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-chgsavf` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_file_prompts |
| `prompts/prompt-chgsavf-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_file_prompts |
| `prompts/prompt-chkopt-refused` | pub400 has no CHKOPT (not found); ours defines it |
| `prompts/prompt-chktap` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-clrsavf` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_file_prompts |
| `prompts/prompt-cpyfrmtap` | deferred: CPYFRMTAP is not defined |
| `prompts/prompt-cpyfrmtap-f10` | deferred: CPYFRMTAP is not defined |
| `prompts/prompt-cpyfrmtap-f10-2` | deferred: CPYFRMTAP is not defined |
| `prompts/prompt-cpyptf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-cpytotap` | deferred: CPYTOTAP is not defined |
| `prompts/prompt-cpytotap-f10` | deferred: CPYTOTAP is not defined |
| `prompts/prompt-cpytotap-f10-2` | deferred: CPYTOTAP is not defined |
| `prompts/prompt-crtdevmlb` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-crtdevmlb-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-crtdevmlb-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts |
| `prompts/prompt-crtdevopt` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-crtdevopt-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-crtdevtap` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-crtdevtap-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-crtdevtap-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts |
| `prompts/prompt-crtimgclg` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-crtimgclg-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-crtsavf` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_file_prompts |
| `prompts/prompt-crtsavf-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_file_prompts |
| `prompts/prompt-dltlicpgm-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-dltprb-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-dmpjob-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-dmpobj-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-dmpsysobj-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-dspbckup` | pending (agent): cleanup/backup commands |
| `prompts/prompt-dspipla` | compared by `command/sys-cpps/tests/phase11_prompts.rs` licpgm_and_system_prompts |
| `prompts/prompt-dspopt` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-dspprb` | compared by `command/sys-cpps/tests/service_prompts.rs` |
| `prompts/prompt-dspprb-2` | compared by `command/sys-cpps/tests/service_prompts.rs` |
| `prompts/prompt-dspprb-f10-3` | compared by `command/sys-cpps/tests/service_prompts.rs` |
| `prompts/prompt-dspptfcvr-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-dspptf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-dspsavf` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_file_prompts |
| `prompts/prompt-dspsfwrsc` | compared by `command/sys-cpps/tests/phase11_prompts.rs` licpgm_and_system_prompts |
| `prompts/prompt-dsptap` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-dupopt` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-dupopt-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-duptap` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-duptap-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts |
| `prompts/prompt-duptap-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts |
| `prompts/prompt-endtrc-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-insptf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-inzopt` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-inzopt-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-inzopt-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts |
| `prompts/prompt-inztap` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-lodimgclg` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-lodptf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-prtsysinf-refused` | refused; deferred: PRTSYSINF is not defined |
| `prompts/prompt-prttrc-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-rcldbxref-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-rmvptf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-rstaut-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *SAVSYS |
| `prompts/prompt-rstcfg-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours checks object authority, *SAVSYS or *ALLOBJ overriding; no command-level refusal (gap) |
| `prompts/prompt-rstlib-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours checks object authority, *SAVSYS or *ALLOBJ overriding; no command-level refusal (gap) |
| `prompts/prompt-rstlicpgm-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-rstobj-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours checks object authority, *SAVSYS or *ALLOBJ overriding; no command-level refusal (gap) |
| `prompts/prompt-rstusrprf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *SAVSYS |
| `prompts/prompt-rst-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours checks object authority, *SAVSYS or *ALLOBJ overriding; no command-level refusal (gap) |
| `prompts/prompt-runbckup` | pending (agent): cleanup/backup commands |
| `prompts/prompt-runbckup-f10` | pending (agent): cleanup/backup commands |
| `prompts/prompt-sav` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savcfg` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savcfg-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savcfg-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savchgobj` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savchgobj-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savchgobj-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savchgobj-f10-3` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savchgobj-f10-4` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savchgobj-f10-5` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savlib` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savlib-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savlib-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savlib-f10-3` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savlib-f10-4` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savlib-f10-5` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savlicpgm-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *SAVSYS or *ALLOBJ |
| `prompts/prompt-savobj` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savobj-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savobj-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savobj-f10-3` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savobj-f10-4` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savobj-f10-5` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savobj-f10-6` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savsavfdta` | deferred: SAVSAVFDTA is not defined |
| `prompts/prompt-savsavfdta-f10` | deferred: SAVSAVFDTA is not defined |
| `prompts/prompt-savsavfdta-f10-2` | deferred: SAVSAVFDTA is not defined |
| `prompts/prompt-savsecdta` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savsecdta-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savsecdta-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-savsys` | deferred: SAVSYS is not defined |
| `prompts/prompt-savsys-f10` | deferred: SAVSYS is not defined |
| `prompts/prompt-savsys-f10-2` | deferred: SAVSYS is not defined |
| `prompts/prompt-sav-f10` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-sav-f10-2` | compared by `command/sys-cpps/tests/phase11_prompts.rs` save_prompts |
| `prompts/prompt-sndptford-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours needs *ALLOBJ |
| `prompts/prompt-sndsrvrqs-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-strclnup` | pending (agent): cleanup/backup commands |
| `prompts/prompt-strtrc-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-trcjob-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-vfyimgclg` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-wrkcntinf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-wrkimgclg` | compared by `command/sys-cpps/tests/phase11_prompts.rs` media_prompts and `command/sys-cpps/tests/media.rs` (labels) |
| `prompts/prompt-wrklicinf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-wrkprb-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-wrkptfgrp-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `prompts/prompt-wrksfwagr` | compared by `command/sys-cpps/tests/phase11_prompts.rs` licpgm_and_system_prompts |
| `licpgm/dspsfwrsc-1` | compared by `ui/uim/tests/system_panels.rs` dspsfwrsc (the same capture as `verified/pub400/dspsfwrsc`) |
| `licpgm/dspsfwrsc-2` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-3` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-4` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-5` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-6` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-7` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-8` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-9` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-10` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-11` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-12` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-13` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-14` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-15` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-16` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-17` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-18` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-19` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-20` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-21` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-22` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-23` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-24` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-25` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-26` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-27` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-28` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-29` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-30` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-31` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-32` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-33` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-34` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-35` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-36` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-37` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-38` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-39` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-40` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-41` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-42` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-43` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-44` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-45` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-46` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-47` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-48` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-49` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-50` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-51` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-52` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-53` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-54` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-55` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-56` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-57` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-58` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-59` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-60` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-61` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-62` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-63` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-64` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-65` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-66` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-67` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-68` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-69` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-70` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-71` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-72` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-73` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-74` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-75` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-76` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-77` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-78` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-79` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-80` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-81` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-82` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-83` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-84` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-85` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-86` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-87` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-88` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-89` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-90` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-91` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-92` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-93` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-94` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-95` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-96` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-97` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-98` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-99` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-100` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-101` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-102` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-103` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-104` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-105` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-106` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-107` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-108` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-109` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-110` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-111` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-112` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-113` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-114` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-115` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-116` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-117` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-118` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-119` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-120` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-121` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-122` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-123` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-124` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-125` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-126` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-127` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-128` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-129` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-130` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-131` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-132` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-133` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-134` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-135` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-136` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-137` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-138` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-139` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-140` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-141` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-142` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-143` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-144` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-145` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-146` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-147` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-148` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-149` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-150` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-151` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-152` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-153` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-154` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-155` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-156` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-157` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-158` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-159` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-160` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-161` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-162` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-163` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-164` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-165` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-166` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-167` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-168` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-169` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-170` | covered by dspsfwrsc-1: a later page of the same list |
| `licpgm/dspsfwrsc-f11` | compared by `ui/uim/tests/phase11_panels.rs` dspsfwrsc_libraries (differs: recorded) |
| `licpgm/menu-licpgm-refused` | refused (menu object); our `GO LICPGM` exists in ui/menus/sys, shipped without a refusal (gap) |
| `licpgm/prompt-dspsfwrsc` | compared by `command/sys-cpps/tests/phase11_prompts.rs` licpgm_and_system_prompts |
| `menus/menu-backup` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-cleanup` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-cmdbckup` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDBCKUP) |
| `menus/menu-cmdclnup` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDCLNUP) |
| `menus/menu-cmddmp` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDDMP) |
| `menus/menu-cmddmp-2` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDDMP) |
| `menus/menu-cmdimgclg` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDIMGCLG) |
| `menus/menu-cmdlicpgm-refused` | pub400 has no such menu; neither do we |
| `menus/menu-cmdmed-refused` | pub400 has no such menu; neither do we |
| `menus/menu-cmdopt` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDOPT) |
| `menus/menu-cmdopt-2` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDOPT) |
| `menus/menu-cmdopt-3` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDOPT) |
| `menus/menu-cmdprb` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDPRB) |
| `menus/menu-cmdprb-2` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDPRB) |
| `menus/menu-cmdptf` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDPTF) |
| `menus/menu-cmdptf-2` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDPTF) |
| `menus/menu-cmdrst` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDRST) |
| `menus/menu-cmdrst-2` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDRST) |
| `menus/menu-cmdrst-3` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDRST) |
| `menus/menu-cmdsav` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDSAV) |
| `menus/menu-cmdsavf` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDSAVF) |
| `menus/menu-cmdsav-2` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDSAV) |
| `menus/menu-cmdsav-3` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDSAV) |
| `menus/menu-cmdsrv` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDSRV) |
| `menus/menu-cmdsrv-2` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDSRV) |
| `menus/menu-cmdsrv-3` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDSRV) |
| `menus/menu-cmdtap` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDTAP) |
| `menus/menu-cmdtap-2` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDTAP) |
| `menus/menu-cmdtap-3` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDTAP) |
| `menus/menu-cmdtrc` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDTRC) |
| `menus/menu-cmdtrc-2` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDTRC) |
| `menus/menu-cmdtrc-3` | compared by `ui/menus/tests/generated.rs` numbering_follows_the_goldens (CMDTRC) |
| `menus/menu-licpgm-refused` | refused (menu object); our `GO LICPGM` exists in ui/menus/sys, shipped without a refusal (gap) |
| `menus/menu-problem` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-problem2` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-ptf` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-ptf-2` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-restore` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-restore-2` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-restore-3` | compared by `ui/menus/tests/task_menus.rs` |
| `menus/menu-save` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-save-2` | compared by `ui/menus/tests/task_menus.rs` (the same capture as `verified/pub400`) |
| `menus/menu-save-3` | pending (agent): GO SAVE lacks option 43 (the cleanup/backup agent builds GO SAVE) |
| `menus/menu-service-refused` | refused (menu object); our `GO SERVICE` exists in ui/menus/sys, shipped without a refusal (gap) |
| `menus/menu-tape` | compared by `ui/menus/tests/task_menus.rs` |
| `displays/dspbckup` | pending (agent): DSPBCKUP |
| `displays/dspbckup-2` | pending (agent): DSPBCKUP |
| `displays/dspipla` | deferred: DSPIPLA answers in text (lic/slic/src/cpp/iplcmd.rs), no panel |
| `displays/dspptf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `displays/wrkcntinf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `displays/wrkimgclg` | pending (agent): the WRKIMGCLG panels |
| `displays/wrklicinf-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `displays/wrkprb-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `displays/wrkptfgrp-refused` | refused; defined (`command/sys-cpps/tests/phase11_prompts.rs` refused_commands_are_defined); ours has no special-authority check (gap: IBM's class is *SERVICE or *ALLOBJ) |
| `displays/wrksfwagr` | compared by `ui/uim/tests/phase11_panels.rs` wrksfwagr (differs: recorded) |
| `displays/wrksfwagr-2` | covered by wrksfwagr: a later page of the same list |
| `displays/wrksfwagr-3` | covered by wrksfwagr: a later page of the same list |
| `savf/dspsavf-1` | compared by `ui/uim/tests/phase11_panels.rs` dspsavf (differs: recorded) |
| `savf/dspsavf-2` | covered by dspsavf-1: a later page of the same list |
| `savf/dspsavf-3` | covered by dspsavf-1: a later page of the same list |
| `savf/dspsavf-4` | covered by dspsavf-1: a later page of the same list |
| `savf/dspsavf-5` | covered by dspsavf-1: a later page of the same list |
| `savf/dspsavf-5-file` | deferred: DSPSAVF has no option 5 |
| `savf/dspsavf-5-jrn` | deferred: DSPSAVF has no option 5 |
| `savf/dspsavf-5-srcfile` | deferred: DSPSAVF has no option 5 |
| `savf/dspsavf-after-help` | not compared: help text is ours by rule |
| `savf/dspsavf-f11` | deferred: DSPSAVF has no F11 view |
| `savf/dspsavf-f16` | deferred: DSPSAVF has no F16 header display |
| `savf/dspsavf-f16-2` | deferred: DSPSAVF has no F16 header display |
| `savf/dspsavf-help` | not compared: help text is ours by rule |
