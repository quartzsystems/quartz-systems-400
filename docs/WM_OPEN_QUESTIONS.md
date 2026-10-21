# Work management open questions

Behaviour of jobs, queues and subsystems that the IBM documentation
leaves open, or that QS/400 does differently for now, with the reading
implemented and the test on the real machine that settles it. An entry
closes when the machine's answer is recorded here and the code agrees.

## Jobs

1. **Job number of `SCPF`.** Implemented: `000000`, and numbers handed
   out from 1 up, wrapping after 999999 and skipping active jobs. The
   machine shows `SCPF` as `000000`; whether the counter skips numbers
   of jobs still on output queues (ended, spooled files kept) is not
   documented. Test: on the machine, `WRKUSRJOB *ALL *OUTQ`, note the
   lowest number held, and watch whether `SBMJOB` ever reuses it after
   a wrap. Implemented: active and queued numbers are skipped; the
   numbers of ended jobs whose spooled files remain are not.

2. **What `DSPJOBLOG` shows of an active job.** Implemented: the
   entries `LOG(level severity *)` keeps, with level 0 read as 1 so the
   display is never empty of messages; the printed log applies the
   level exactly. Test: `CHGJOB LOG(0 99 *NOLIST)`, send a message to
   `*EXT`, `DSPJOBLOG`: whether the machine shows it.

3. **Severity of a request message in the job log.** Implemented: 00.
   The machine's `QPJOBLOG` shows requests as `Request ... 00`; this
   matches every log seen, but is not documented.

## Messages

4. **Break delivery inside a running program.** Implemented: a job
   takes its break messages at its wait points (data queue and message
   waits, the console's idle wait) and when each request returns. The
   machine interrupts a program at the next machine-instruction
   boundary. Here that needs a check at every statement of every
   program, which generated code has only in its debug variant; the
   cost of adding it everywhere is not yet measured. Test on the
   machine: a CL program looping `CHGVAR` for ten seconds with no
   waits, a break message sent meanwhile: it is shown during the loop.
   QS/400 shows it when the call ends.

5. **`MSGQFULL(*WRAP)` and unanswered inquiries.** Implemented: wrap
   removes the oldest messages that are not inquiries waiting for a
   reply; a queue full of unanswered inquiries rejects the new message
   with `CPF2460`. Test: a queue of `SIZE(1 1 1) MSGQFULL(*WRAP)`
   filled with inquiries, then one more message.

## Subsystems

6. **History messages for subsystem start and end.** Implemented:
   `CPF0902` and `CPF0903`, with the subsystem and library as data.
   The identifiers the machine writes to `QHST` for these are not
   confirmed. Test: `STRSBS QGPL/QTESTSBS`, `ENDSBS QTESTSBS`, then
   `DSPLOG MSGID(*ALL)` over that minute: note the identifiers.

## Spooling and printing

7. **A DDS printer record with no spacing keyword.** Implemented: it
   prints on the current line and spaces one line after, as if it had
   `SPACEA(1)`. Test: a printer file whose record has fields at fixed
   positions and no `SKIPx`/`SPACEx`, written three times; `CPYSPLF
   CTLCHAR(*PRTCTL)` shows where each line landed.

8. **Option indicators in a printer record without `INDARA`.**
   Implemented: the record buffer starts with one byte (`1` on) for
   each indicator the format uses, in ascending order. The machine's
   layout, as seen by a program using a record buffer directly
   (`QSPPUTSP`-style, or an ILE C `_Rwrite`), is not documented here.
   Test: an ILE C program writing a format conditioned by indicators 50
   and 51, the buffer dumped.

9. **FCFC channels.** Implemented: `1` skips to line 1, `2`-`9`, `A`,
   `B` to lines 2 to 11, `C` to the overflow line; the machine takes the
   channel lines from `CHLVAL`, which is not supported. Test:
   `CRTPRTF CTLCHAR(*FCFC) CHLVAL((1 (3)) (12 (50)))` and records with
   `1` and `C`.

10. **The SCS command forms.** Implemented: the subset in `WORKMGMT.md`,
    with one-byte parameters for `SHF`, `SVF` and `SLD` and `SCD` as
    `2B D2 04 29 00 n`. Test: a spooled file of a program-described
    report from the machine, read raw with `QSPGETSP` (or `CPYSPLF` to
    a stream file as `*SCS`), compared byte for byte with ours.

11. **Overrides issued by a program.** Implemented: `OVRDBF` and
    `OVRPRTF` from command level reach every open; an override's
    `OVRSCOPE(*ACTGRPDFN)` from a named activation group reaches that
    group's opens only. A program's `QCMDEXC` commands run after the
    call returns until the command analyzer of Phase 7, so call-level
    scoping inside a group is not reached yet. Test: CL issuing
    `OVRPRTF ... OVRSCOPE(*ACTGRPDFN)` then calling an RPG program in
    another named group that opens the file.

## Statistics

12. **DB and non-DB faults.** Implemented: a pool's cgroup's major
    faults are its DB faults and pages, its other faults the non-DB
    ones. The machine counts by what the page held. Test: none on
    the machine settles a mapping onto Linux; recorded so the numbers
    are read as what they are.

13. **`QWCRSSTS` formats `SSTS0400` and `SSTS0500`.** Implemented as
    `WORKMGMT.md` lists them: the `SSTS0300` header and 104-byte pool
    entries. Test: `QWCRSSTS` with each format on the machine, the
    receiver dumped and compared field by field.

## System jobs and power down

14. **What `QSTRUP` starts.** Implemented: `QSPL`, `QSERVER`,
    `QUSRWRK`; `QINTER`, `QBATCH`, `QCMN` when `QCTL` controls; the
    writers when `QSTRPRTWTR` is 1; `QGPL/QS36EVOKE` released. Test:
    `RTVCLSRC QSYS/QSTRUP` on the machine.

15. **`PWRDWNSYS IPLSRC`.** Implemented: only `*PANEL`; the slot the
    next IPL uses is the panel's (`qs400.slot`, DST). The machine
    switches the IPL source. `MIGRATE(phase6)`, with the system
    service tools.
