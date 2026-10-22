# Security oracle kit

CL programs for the real IBM i that settle the open questions of
`docs/SEC_OPEN_QUESTIONS.md`. Each observation is one line, `Qn NAME:
text`, written through `SORACLE` into `SECORA/ORACLE`; the file ends up
as `expected/sec-oracle.txt` here, and `cargo test -p conformance
--test sec_oracle` compares every line it knows with this
implementation's reading. Without the file the test reports itself
skipped. Whitespace runs are folded before comparing.

To capture, signed on as `QSECOFR` with `*ALLOBJ` and `*SECADM`:

1. Upload the members to a source file; `CRTBNDRPG SORACLE`,
   `CRTBNDCL SECORACLE`, `CRTBNDCL ADOPTQ2 USRPRF(*OWNER)`, `CRTBNDCL
   ASUSRA`, `CRTBNDCL ASOWNA`.
2. `CALL SECORACLE`: it creates library `SECORA`, the profiles `OWNA`,
   `GRPA`, `USRA` (password `ORACLE1`), the file `SECORA/F` and the
   authorization list `SECORAL`, makes `OWNA` own `ADOPTQ2`, and records
   questions 1, 6, 11 and 12.
3. Sign on as `USRA` (`ORACLE1`) and `CALL SECORA/ASUSRA`: questions 2,
   4, 5, 7, 9 and the `USRA` side of 12. Sign on as `OWNA` (`ORACLE1`)
   and `CALL SECORA/ASOWNA`: question 3.
4. `CPYTOIMPF FROMFILE(SECORA/ORACLE) TOSTMF('/tmp/sec-oracle.txt')
   STMFCODPAG(*PCASCII) RCDDLM(*LF) DTAFMT(*FIXED)`; save it as
   `expected/sec-oracle.txt`.
5. For question 10, `CRTBNDCL USRI0300` and `CALL SECORA/USRI0300`: it
   calls `QSYRUSRI USRI0300` for `USRA` and `DMPCLPGM` dumps the receiver
   in hex to spooled file `QPPGMDMP`; keep it as `expected/usri0300.txt`.
   The comparison is by hand. For question 1 beyond the four profiles
   the driver records, `DSPUSRPRF USRPRF(*ALL) TYPE(*BASIC)
   OUTPUT(*OUTFILE) OUTFILE(SECORA/USRPRF)` and `CPYTOIMPF` it to
   `expected/usrprf.csv`.
6. `DLTLIB SECORA` and `DLTUSRPRF` the three profiles afterwards.

| Question | Program | What it records |
|---|---|---|
| 1 | `SECORACLE` | `RTVUSRPRF SPCAUT` of `QSYSOPR`, `QPGMR`, `QSRV`, `QTCP` |
| 2 | `ASUSRA` → `ADOPTQ2` | whether `CHKOBJ *CHANGE` on `F` succeeds inside a program adopting `OWNA`, whose group `GRPA` has `*CHANGE` and who has none |
| 3 | `ASOWNA` | `CHKOBJ *CHANGE` as `OWNA` owning `F3` with its own authority reduced to `*USE` and `GRPA` holding `*ALL` |
| 4 | `ASUSRA` | the message `CHGPWD` sends for a password breaking `QPWDMINLEN` |
| 5 | `ASUSRA` | the message `CRTUSRPRF` sends without `*SECADM` |
| 6 | `SECORACLE` | the owner and `*PUBLIC` of a profile just created |
| 7 | `ASUSRA` | `RUNPTY` after `CHGJOB RUNPTY(10)` in a job whose user has `PTYLMT(5)` |
| 9 | `ASUSRA` | `ADDPFM` on `F` as a user with `*OBJOPR *OBJALTER` and no `*OBJMGT` |
| 11 | `SECORACLE` | whether `QSYS/QSYCHGPR` exists |
| 12 | both | the identifiers of `DLTUSRPRF` of an owner, `GRTOBJAUT` beyond one's own, `ADDAUTLE` of a user already on the list, `RMVAUTLE` of one who is not |

Question 8 (level 20 to 30) needs two IPLs and is not scripted.

## Captured 2026-10-22 on IBM i 7.1 (V7R1M0, QSECURITY 40, QPWDLVL 0)

The box had no RPG compiler, so `SORACLE.CLLE` stood in for
`SORACLE.RPGLE`: the lines went to message queue `SECORA/ORACLE` and
`DSPMSG OUTPUT(*PRINT)` gathered them into `expected/sec-oracle.txt`.
`expected/usri0300.txt` is the `DMPCLPGM` of `USRI0300`,
`expected/usrprf.csv` the `DSPUSRPRF USRPRF(*ALL) TYPE(*BASIC)` outfile
with its column names on the first line. Question 4 goes through
`QSYCHGPW`, since `CHGPWD` has no parameters; question 7 uses
`PTYLMT(5)`, the field's range being 1 to 9. Question 8 was not run.
Answers that differ from this implementation's: QTCP's special
authorities, the identifiers of questions 4, 5 and 12, `CHGJOB RUNPTY`
below the limit is refused (`CPF1337`), `ADDPFM` with `*OBJALTER` is
allowed, and `QSYCHGPR` exists.
