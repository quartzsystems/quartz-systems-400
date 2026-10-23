# Developer tools

Phase 8's programmer tools: what each shows, the keys it takes and what
it does not do yet. Each tool is a library crate under `devtools/` (pure
logic, unit-tested on the host) and a job-side driver under
`work/qsjob/src/devtools/` that draws the screens, reads the keys and
reaches the store through the storage service. IBM's tool screens are
display files, not UIM panels: their keys and legends sit on fixed
columns, so each tool builds its screens as `uim::Screen` values or
shipped DDS in `ui/dspf/sys/`, and tests them against pub400 goldens
(`tools/conformance/golden/verified/pub400/`).

| Tool | Crate | Driver | Storage requests | Goldens |
|---|---|---|---|---|
| SEU | `devtools/seu` | `devtools/seu.rs`, `seu/` | 300-302 | 80 `seu-*` |
| PDM | `devtools/pdm` | `devtools/pdm/` | 320-339 | 32 `pdm-*` |
| DFU, RLU | `devtools/dfu` | `devtools/dfu/`, `rlu.rs` | 360-363 | 24 `dfu-*`, 4 `rlu-*` |
| STRDBG | `devtools/dbg` | `devtools/dbg/` | 380-387 | 52 `dbg-*` |
| STRSQL | `devtools/isql` | `devtools/isql/` | 400-404 | 37 `strsql-*` |
| STRSST | `devtools/sst` | `devtools/sst.rs` | 420-432 | `golden/dst/`, `golden/sst/` |

## SEU

`STRSEU SRCFILE SRCMBR TYPE OPTION TEXT`. Option 2 edits, 5 browses
and 6 prints to `QSYSPRT`. `*PRV` names the last member the job used.
A member that does not exist opens empty and is added when saved. The
storage service locks the member `*EXCL` for edit and `*SHRRD` for
browse, and refuses another job with "member in use" (CPF3330).

**Screens.** The edit and browse display is QSUSEU. Row 1 shows
Columns, the mode and the file, or `Pending` while a command waits.
Row 2 holds `SEU==>` and the member. Row 3 is the format line. Rows
4-20 hold records with sequence and data fields, and rows 22-23 the key
sets that F24 cycles. The window is 71 columns wide and starts at the
source type's first column (6 for RPG). F19/F20 move it 36 columns;
F11 toggles to column 31 and back. A record's date shows five columns
after the data when the record's end is in the window. The other
displays are Change Session Defaults (QSUDFT, two pages in edit mode,
one in browse), Find/Change and Find Options (QSUFND), Browse/Copy and
Browse Options (QSUBRW), Exit (QSUEXIT), the system command window
(QSUSYS), Select Prompt (QSUSEL) and the fixed-form prompts, drawn
from column tables under the records.

**Line commands.** Typed over the sequence number; the characters that
differ from it are the command, as IBM reads them. A B O On OO, C Cn
CC, CR CCR, M Mn MM, D Dn DD, I In, IP IPxx, IF IFxx, RP RPn RPP, L Ln
LL, R Rn RR, X Xn XX, S Sn SS SF SL, F Fn FF Fxx, P Pn PP Pxx, W Wn,
T Tn TT, + - +n -n, and a sequence number to position. A source or
block end waits, shown in its sequence area and as `Pending` in row 1,
until its partner comes. Inserted lines number by the insert
increment, 1.00 at the end of the member; when there is no room, later
records are renumbered. Typing into an insert line keeps insert mode
going.

**Command line.** FIND/F, CHANGE/C (string, NEXT PREV FIRST LAST ALL,
X NX, columns), TOP/T, BOTTOM/B, SAVE and FILE (`[lib/]file
[member]`), CANCEL/CAN, EXIT, SET CAPS/TABS/MATCH/FULL/ROLL, a
sequence number, +n and -n.

**Keys.** F3 Exit (leaves at once in browse mode), F4 prompt, F5
refresh, F9 retrieve, F10 cursor, F11 toggle, F12 cancel, F13
defaults, F14 find options, F15 browse options, F16 repeat find, F17
repeat change, F19/F20 shift, F21 system command, F23 Select Prompt,
F24 more keys, and roll by Amount to roll (H, F, C, D or a number).

**Prompts.** CL goes to the command prompter with a Label and a
Comment field, and the result is written back over the statement's
records (label from column 2, command at 14, parameters at 25, `+`
continuation). RPG H, F, D, I, C, CX, O and P and the DDS prompts are
built from column tables. Other types get the free format with a ruler.

**Syntax checking** covers added or changed records within the
session's range. CL goes through the analyzer as a program statement,
command definitions through the definition compiler, DDS through its
type's compiler, and RPG has a hook (`TODO(phase9)`). With records in
error, Exit offers Return to editing Y.

**Upper case and dates.** Upper case follows the source type (CL, CMD,
DDS and RPG/400 forced upper; RPGLE mixed; unknown types use the
session default). A changed record takes today's date, and Set records
to date re-dates every record.

**Known gaps.** Split-screen browse (the other member is shown full
screen instead), tab stops, the F4 list on Exit, "Go to member list"
(PDM), recovery after an abnormal end, the CMD and PF/LF prompt
layouts (no golden), field help.

## PDM

`STRPDM` shows the Programming Development Manager menu: 1 libraries, 2 objects, 3 members, 9 user-defined options, a command line, F18 Change Defaults. Each option asks for its values (Specify Libraries/Objects/Members/Option File to Work With) and opens a list; `WRKLIBPDM`, `WRKOBJPDM` and `WRKMBRPDM` open the lists directly, with `*PRV` meaning the user's last values.

**Lists.** Header fields (list type; library; file and library) can be typed over to switch lists; Position to (and Position to type for objects) moves the page. F23 steps through the option sets and F24 through the key sets, as IBM's lists do (libraries 2/3, objects 3/3, members 2/3). F11 shows names and types in columns; F14 shows member change dates or object sizes; F15 sorts members by date; F13 copies the first option typed to every entry below it; F17 subsets (name, type, attribute, text, dates); F16 opens the user-defined options; F18 changes defaults; F21 prints the list to `QSYSPRT`; F6 creates (`CRTLIB`, `GO CMDCRT`, `STRSEU` on the file); F9 retrieves the last command.

**Options.** The `pdm` crate maps each option, per list and type, to a command or an action: confirm (4), rename (7), new text (13), the next list (12), compile (14, 15), find string (25). Parameters on the command line are added to the option's command, and F4 prompts it. Options 2, 5 and 6 on members call `STRSEU` with `OPTION(2|5|6)`; 17, 18 and 19 call `STRSDA`, `UPDDTA` and `STRRLU`.

**Compile.** CLLE `CRTBNDCL` (15: `CRTCLMOD`), CLP `CRTCLPGM`, PF `CRTPF`, LF `CRTLF`, DSPF and MNUDDS `CRTDSPF`, PRTF `CRTPRTF`, CMD `CRTCMD`, PNLGRP `CRTPNLGRP`, MENU `CRTPNLGRP` then `CRTMNU TYPE(*UIM)`. RPG, COBOL and C types are known and refuse with "compiler not installed" until Phase 9. The object goes to the default object library (`*CURLIB`, `*SRCLIB` or a name), with `REPLACE` from the defaults, through `SBMJOB` when "Compile in batch" is Y.

**Substitution.** `&L` library, `&F` file, `&N` name, `&O` object library, `&T` type, `&A` attribute, `&B` list type, `&C` option, `&E` run in batch, `&H`/`&J` job description and library, `&R` replace, `&U` user, `&X` quoted text; `&&` is one ampersand.

**User-defined options.** One- or two-character codes not used by PDM's own options, kept as records of a source member (code in columns 1-2, command from column 4), by default `QGPL/QAUOOPT` member `QAUOOPT`, written by the system for everyone. A new system shows eight Quartz options until the file is first written. Work with User-Defined Options creates (F6), changes, copies, deletes and displays them, writing the file when the list is left (F15 discards the changes).

**Defaults.** Object library, replace, compile and run in batch, save session defaults, save/restore option, job description, change type and text, option file, full screen mode, log option commands, exit lists on Enter. They are stored with the `*PRV` values in member `<user>` of `QUSRSYS/QAUODFT`.

**Find string.** Option 25 and `FNDSTRPDM` search the members' source between two columns, matching case or not. They then run an option (or a user-defined one) on each member that holds the string, prompted if asked, and print the list or the matching records. Without a display, `FNDSTRPDM` gives the report as its output.

**Known gaps.** F10 does not start Command Entry from PDM. No file list for F4 on Specify Members. The library list has no subset panel. Member option 3 prompts `CPYSRCF` instead of IBM's Copy Members panel. 54 and 55 (`CMPPFM`, `MRGSRC`), 9 and 10 (`SAVOBJ`/`RSTOBJ`) and 16 on REXX members have no command behind them yet. No help on the lists. Print format, mark record and overflow on Find String are accepted but not used.

## DFU

`STRDFU` shows the menu (Run, Create, Change, Delete a DFU program,
Update data using temporary program) and its entry panels, which
remember their values for the session. `UPDDTA`, `CHGDTA` and
`DSPDTA` go straight to the data. DFU programs are `*PGM` objects of
attribute `DFU` whose payload is the definition as text. Create and
Change go through Define General Information (Nonindexed or Indexed
File); Enter stores the program, and F12 asks once before leaving.
Records go through the open data path the service keeps for the job.

**Working with data.** Title and mode (CHANGE, ENTRY, INSERT,
DISPLAY) on row 1, format and file on row 2, the record number or key
fields on row 4, the fields from row 5 with values at column 11 (or
after a longer label). A value runs past column 80 onto the next row,
numbers are right-adjusted with the sign last, and pages turn when the
fields don't fit.

**Keys.** F3 End Data Entry (the counts, Y/N), F5 refresh, F6 Select
Record Format, F9 insert, F10 entry, F11 change, Page Down/Up to roll
(the record is written first when it was changed), F23 delete the
record shown.

**Known gaps.** No format or field selection displays, no audit
report, no running a DFU program by `CALL`; the creation displays after
Define General Information.

## RLU

`STRRLU` shows Design Report: `Columns`, the `RLU==>` line, a ruler,
the page banners, and one image line per printer line, with fields as
`X`/`9` masks and the format name in the prefix area. Typing changes
constants; a field remains while its mask is whole. Line commands `I`,
`D`, `DR`; F19/F20 shift; F24 cycles three key sets; `SAVE`, `FILE`,
`CANCEL`. Exit RLU: 1 saves (and creates the printer file when asked),
2 leaves, 3 resumes. `OPTION(5)` browses, `OPTION(6)` prints a
listing. The member is read and written through PDM's requests.

**Known gaps.** F11, F13-F18, F22 and F23 answer "not in this RLU";
no prototype report or batch; the DDS is regenerated, so comments and
the source's own line and position style are lost.

## STRSQL

Statements run in the job's own SQL context (naming and commitment
control from the session attributes); sessions are saved per user in
the user space `QUSRSYS/QSQLSESS`. In batch `STRSQL` fails and points
to `RUNSQLSTM`.

**Screens.** Enter SQL Statements (both key pages, `*SYS` and `*SQL`
key texts), Select SQL Statement (3 pages), the seven statement
prompts, Session Services, both pages of Change Session Attributes,
Exit Interactive SQL, Display Data, and the Libraries, Files and
Fields select lists.

**Keys.** F3 and F12 lead to Exit (options 1-4), F4 prompt, F6 insert
line, F9 retrieve, F10 copy line, F13 services, F14 delete line, F15
split line, F16-F18 the lists (names go in at the cursor), F24 more
keys, the roll keys over the history, F1 on an SQL message for its
second-level text.

**History.** The scrolling area is rows 4-20. A statement is kept as
the lines it was typed on, shown after `>` at column 5; messages are
cut to 68 columns. After a statement runs, the area keeps its position
and scrolls only enough to leave 4 input lines. After an error the
statement is put at the top and stays on the input. `*SYN` and `*VLD`
only parse.

**Display Data.** A column is as wide as its type or its heading,
whichever is wider; a value narrower than its heading is centred under
it; the scale numbers end on their column; F19/F20 shift by 79.
Output 2 prints to `QSYSPRT`.

**Known gaps.** Output to a file (3), F21 split on Display Data, F4
lists inside the prompts; pending changes are committed, not asked
about, when the session ends.

## STRDBG

`STRDBG`, `DSPMODSRC` and `ENDDBG` at a display; a job without one
keeps the Phase 4 text debug session.

**Screens.** Display Module Source (row 3 program, library and module;
source rows 4-18; `Bottom` on 19; `Debug . . .` 144 characters over
rows 20-21; keys on 22-23 in three pages that F24 cycles), Work with
Module List (F14), Work with Watch (F18), Work with Module Breakpoints
(F13), Select View (F15), the command window (F21, rows 18-24), help
(F1 and `HELP`), Evaluate Expression (every command and its answer, 14
rows a page; shown for `DISPLAY EQUATE` and long values), Work With
Debugged Threads (`THREAD`), Display Breakpoints (`DSPBKP`), Display
Breakpoint (a stop at an `ADDBKP` breakpoint).

**Keys.** F3 leaves (at a stop: runs to the end), F5 refresh, F6
add or clear a breakpoint at the cursor's line, F9 retrieve, F10/F22
step over and into (not stopped: the next statement stops), F11
display and F17 watch the variable at the cursor, F12 resume or leave,
F13, F14, F15, F16 repeat find, F18, F19/F20 shift by 40, F21, F23
program output, F24, Page Up/Down.

**Commands.** BREAK, TBREAK, CLEAR, DISPLAY, EVAL, WATCH, STEP, ATTR,
QUAL, FIND, UP, DOWN, LEFT, RIGHT, TOP, BOTTOM, NEXT, PREVIOUS, HELP,
SET, THREAD, EQUATE, spelled and abbreviated as IBM's. A failed command
stays on the line with its first word in upper case. Without a source
view, BREAK and CLEAR numbers are statement numbers. EQUATE names
expand at the start of a line. `WHEN` compares a variable with a
value, numerically when both read as numbers.

**STRSRVJOB.** The servicing job posts STRDBG, the breakpoints, resume
and EVAL to the serviced job through the storage service (a mailbox
each way). The serviced job takes them at CALL, at its one-second
break poll and while stopped; its stops show in the servicing job's
Display Module Source, which goes on watching after a resume.

**Known gaps.** `EVAL` assignment; F3 does not end the program; one
thread (TBREAK is an ordinary breakpoint); watches only in your own
job; a stopped serviced job waits for `ENDSRVJOB` if its servicer
ends; UP/DOWN/LEFT/RIGHT need no argument (IBM's do).

## STRSST

Needs a display and `*SERVICE` (CPF2222 otherwise). STRSST runs
DST's own panels (`lic/svctools`) in the job, converted to
`uim::Screen`s; the job's environment asks the storage service, which
checks `*SERVICE` and the signed-on service tools user's privileges on
every request.

**Screens.** Start Service Tools (STRSST) Sign On, Change Service Tools
User Password, System Service Tools (SST) with options 1, 2, 3, 6, 7
and 8 (4 and 5 answer "not available"), then DST's panels: Start a
Service Tool (product activity log, Display/Alter/Dump as display
only, LIC log as the IPL history, hardware service manager), Work with
Disk Units, Work with System Security, Work with Service Tools User
IDs; plus Work with Active Service Tools, Work with System Capacity
(read-only processor status) and Work with Service Tools User IDs and
Devices.

**Keys.** Enter, F3, F12, F6 (create user), F7/F8 and Page Up/Down on
displays, F10 to confirm a change: every change to disks, security
options or user IDs asks first.

**Known gaps.** No Command Entry from SST, no device IDs, diskette
recovery or partitions, no progress display during long disk actions.
An SST sign-on is audited as `SS` (a code of ours).
