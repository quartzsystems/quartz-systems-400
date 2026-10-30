# Commands and CL: open questions

Readings a real IBM i would settle. Each names what this system does
now and what to check.

1. **Prompt displays.** The goldens under `command/cmd/tests/golden/`
   and `command/sys-cpps/tests/golden/` are our renders: label column
   2, dots to column 33, the `>` marker in 35, the field in 37, the
   choices from 51, 16 parameter rows per page. IBM's exact columns,
   the choice texts for ranges and special values, and where a long
   prompt text wraps are unverified. A capture of F4 on CRTPF, CRTLIB,
   SBMJOB and CHGJOB on the real box would fix them.

2. **The analyzer kit.** Settled by the capture of 2026-10-22
   (`tools/conformance/cmd/oracle.txt`, IBM i 7.1): the identifiers
   `docs/CMD.md` lists, the order the diagnostics come in, which
   definitions take generic names or special values, and the
   validity-checker messages `DEP` statements now send. 20 of the 797
   strings still differ; item 15 names each.

3. **Three-part qualified names.** Settled: `JOB(A/B/C/D)` is
   `CPD0045` on the box, so a job name has exactly three parts; a file
   member stays an `ELEM` beside its qualified file.

4. **`EXPR(*YES)`.** Partly settled: the box refuses an expression on
   a `*DATE` (`SCDDATE(13/45/26)` is `CPD0104`) and accepted every
   other expression the kit gave it, which is what this analyzer does.
   A parameter where IBM refuses `'a' *CAT 'b'` is still unknown; the
   kit has no such string.

5. **Return variable lengths.** Partly settled: `RTVSYSVAL` without
   `RTNVAR` is `CPD0071`, so a return variable may be required (the
   definition compiler now allows `MIN` with `RTNVAL(*YES)`). The
   length rule is unverified: `QCMDCHK` has no declarations, so the
   capture never compared a variable's length with the parameter's.

6. **`MONMSG CMPDTA`.** Compared with the first characters of the
   message data, as IBM documents; a mismatch passes the message to
   the statement's next `MONMSG` that names it, else it is unmonitored.
   Where the data starts for the common messages (CPF9801's object
   name at offset 0?) decides which captures match.

7. **The direct path's block.** When a CL program calls a system
   command's processing program directly, IBM passes exactly the
   layout `docs/CMD.md` §4 describes for every type; whether IBM
   passes `*INT2` as a 2-byte or a 4-byte field in the block, and
   whether a `PASSATR` byte precedes a list, is read from the
   documentation, not a dump.

8. **Command Entry.** Our panel: the previous commands and messages on
   rows 5 to 18, `Bottom`/`More...` at row 18 column 73, the command
   line on rows 20 to 22, the keys on 23 and 24, F9 cycling the last
   25 commands, F10 adding second-level text under each message, F11
   wrapping long lines. IBM's row for the roll indicator, the exact
   key words and what F11 shows are unverified against a capture.

9. **`QCMDEXC` from a program at `*PRV`.** A message sent to `*PRV`
   by the first program of a job goes to the external queue, and an
   escape there ends the job's call with the exception pending. IBM
   ends the request and shows the message at Command Entry; the
   inquiry a batch job gets instead is unverified.

10. **Eight-digit dates.** Unverified: the kit has no eight-digit
    date; the box reads an unquoted date with separators as an
    expression (`CPD0104`), which the analyzer now does too.

11. **The listing.** Our listing's column layout (statement number,
    nesting, source; the cross-reference columns) imitates IBM's shape
    with our words. A real `CRTBNDCL` listing would show the line
    widths and whether the cross-reference lists subroutines.

12. **Message identifiers of the cleanup's commands.** `RUNSQL` ends
    in `SQL9010` and `RUNSQLSTM` past `ERRLVL` in `SQL9006`; `ALCOBJ`
    refuses with `CPF1002`, a missing message file is `CPF2407`, a
    missing message `CPF2419`, a missing device description `CPF2702`,
    a missing member `CPF9815`, a production file opened for update
    under `UPDPROD(*NO)` `CPF4188`. All from recollection; the job logs
    of the real box settle them.

13. **`ALCOBJ WAIT`.** The service answers every job on one thread, so
    a conflicting lock is refused at once whatever `WAIT` says. The
    machine waits. Test: `ALCOBJ` with `WAIT(10)` against a held object
    released after five seconds.

14. **`CHGNETA SYSNAME`.** Takes effect at once and sets `QSYSNAME`;
    the machine keeps it pending (`PNDSYSNAME`) until the next IPL.

15. **What the analyzer kit's capture left.** The 23 strings whose
    outcome still differs from the box, with the reason:
    - `?CRTLIB`, `CRTLIB ?LIB(ABC)`, `CRTLIB ?*LIB(ABC)`,
      `CRTLIB ?<LIB(ABC)`, `??CRTLIB`: `QCMDCHK` refuses prompting
      (`CPD0037`, `CPD0016`); the analyzer prompts, as a command line
      must.
    - `CRTJRN ... MNGRCV(*USER) DLTRCV(*NO)`, `ENDRCV RCDFMT(PROMPT)`,
      `CRTMSGQ ... ALWALT(*NO) MSGQFULL(*WRAP)`: one keyword of each is
      not in the 7.1 definition (`CPD0043`); which one needs the
      definitions themselves.
    - `ENDSBS ... DELAY(*NOLIMIT) ... BCHTIMLMT(*NONE)`: one of the two
      special values is not one on the box (`CPD0076`).
    - `ENDJOB ABC/ABC/*` positional: `CPD0017` there, `CPD0012` here.
    - `WRKSPLF SELECT((...)) PERIOD(((...)))`: the box accepts the
      extra parentheses on these two groups and not on others.
    - `WRKSPLF SELECT(*CURRENT *ALL *ALL *ALL) JOB(*)`: `CPD0079`
      there; the value it names is not known.
    - `SNDPGMMSG MSGID(CPF9898)`: `CPD2441`, a validity check whose
      rule is not known.
    - `IF (&A *EQ 1) GOTO X`: the box lets a positional `THEN` take the
      rest of the line.
    - `WAIT DEV(*FILE)`: `CPD0081` there.
    - `CRTPRXCMD CMD(...)`: IBM's keyword is `CMD`, ours `PRXCMD`; the
      processing program in `lic/slic/src/cpp/direct.rs` reads
      `PRXCMD` and must change with the definition.
    - `CRTDTAQ ... RMTLOCNAME(*RDB) RDB(REMOTE)`: `CPD9266`, a validity
      check of the DDM data queue.
    - `CRTPF RCDLEN(32767)` is `CPD0091` and `RCDLEN(0)` `CPD7347`
      there, `CPD0085` here.
    - `DSPFD TYPE(*MBRLIST *ACCPTH) OUTPUT(*OUTFILE)`: `CPD3003`, one
      `TYPE` with an outfile; a `DEP` cannot count a list.
    - `MOVOBJ ... TOLIB(QTEMP)` is now `CPD0090` here too, read from
      the capture rather than the documentation.
    - `CRTBNDCL`, `CRTCLMOD` and `CRTCMD` with `SRCSTMF`: the keyword
      came after 7.1 (`CPD0043` on the box); Phase 10 added it as later
      releases have it.
