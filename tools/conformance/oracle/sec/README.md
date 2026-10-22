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
| 7 | `ASUSRA` | `RUNPTY` after `CHGJOB RUNPTY(10)` in a job whose user has `PTYLMT(50)` |
| 9 | `ASUSRA` | `ADDPFM` on `F` as a user with `*OBJOPR *OBJALTER` and no `*OBJMGT` |
| 11 | `SECORACLE` | whether `QSYS/QSYCHGPR` exists |
| 12 | both | the identifiers of `DLTUSRPRF` of an owner, `GRTOBJAUT` beyond one's own, `ADDAUTLE` of a user already on the list, `RMVAUTLE` of one who is not |

Question 8 (level 20 to 30) needs two IPLs and is not scripted.
