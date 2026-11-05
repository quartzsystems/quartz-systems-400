# Table integrity on the primary (V7R1M0), 2026-11-05

Probes run over JDBC (`jdbc:as400`, `*SQL` naming), record-level access
(JTOpen `KeyedFile`, which goes through DDM) and `CommandCall`, in library
QSTEST, which is not an SQL schema (no QSQJRN: every CREATE TABLE warns
SQL7905). Only message IDs, SQLCODEs and SQLSTATEs are kept; IBM's texts
are not. The SQL outcomes are also the kit's captures in
`tools/conformance/sql/expected/integrity/`.

## Record-level access (DDM)

| Operation | Messages |
|---|---|
| write a duplicate primary or unique key | CPF5009, CPF5034 (DDM's; a local program gets CPF5026) |
| write breaking a check | CPF9898 (DDM wraps the check violation; locally CPF502F) |
| write a foreign key with no parent | CPF502D |
| delete or update a parent key with dependents (RESTRICT) | CPF503A |
| write to a parent whose dependents have CASCADE/SET NULL/SET DEFAULT, files not journaled | CPF4328 |
| write to a file in check pending | CPF523B |

## Commands

- `ADDPFCST TYPE(*PRIKEY|*UNQCST|*CHKCST)`: CPI32B0 then CPC32B0 (n added).
- `ADDPFCST TYPE(*REFCST)` with the parent not journaled: CPI32B0, CPI32B4,
  CPC32B0; a second referential constraint between the same files adds
  CPI32B5 first.
- `ADDPFCST ... PRNKEY(*PRIKEY)` is refused: CPD0078, CPF0001 (PRNKEY takes
  field names; leaving it out means the primary key).
- `ADDPFCST` on a missing file: CPF9812, CPF32B0.
- Generated names: `Q_<lib>_<file>_<first key field>_00001` for keys,
  `Q_<lib>_<file>_00001` for a check.
- `CHGPFCST STATE(*DISABLED)`: CPI32B2, CPC32B2. `STATE(*ENABLED)` over
  records that break it: CPD32C5, CPD32B7, CPI32B2, CPC32B2, and the
  constraint is in check pending (SYSCST CHECK_PENDING = YES). `CST(*ALL)`
  sends one CPI32B2 per constraint and `CPC32B2 3 constraint(s)`.
  `CHKPND` is not a CHGPFCST keyword (CPD0043).
- A file in check pending: every SQL statement on it (SELECT included)
  gets SQL0971 / 57011.
- `RMVPFCST CST(NOPE)`: CPD32B1 and CPC32B1 "0 constraint(s)", not an
  escape. `RMVPFCST CST(*ALL) TYPE(*PRIKEY)` while foreign keys depend on
  the key: CPD32B1, CPC32B1 (0). `CST(*ALL)`: one CPI32B1 each, CPC32B1 n.
- `DSPCPCST` on a constraint not in check pending: CPD32B6, CPF32B6.
- `DSPFD TYPE(*CST) OUTPUT(*OUTFILE)` makes a QAFDCST-format file with the
  fields CSRCEN CSRDAT CSRTIM CSFILE CSLIB CSFTYP CSFILA CSMXD CSFATR CSSYSN
  CSASP CSRES PHCSTN (VARCHAR 258) PHCSTL PHCSTT (R for referential) PHCCKP
  (N) PHCSTS (EE established/enabled) PHNMKY PHKEYL PHCPFN PHCPLN PHPRKY
  PHFRKY (VARCHAR 1200) PHDLTR PHULTR PHTRUN PHCKXP (VARCHAR 2000) PHDFRM PHDFWN.

## SQL

- Two null keys never repeat a unique constraint's key.
- DROP PRIMARY KEY (CASCADE by default) drops the foreign keys over it;
  DROP CONSTRAINT pk RESTRICT with dependents: SQL0616 / 42893; DROP
  TABLE parent RESTRICT: SQL0478 / 42893; DROP TABLE with neither drops the
  dependents' foreign keys.
- ON UPDATE CASCADE: SQL0104 (expects RESTRICT). NO ACTION reports 23504,
  RESTRICT 23001 (SQL0531, SQL0532).
- ALTER TABLE DROP COLUMN (RESTRICT or CASCADE) in a JDBC server job ends
  with SQL0952 reason 10: the CPA32B2 inquiry (data may be lost) is
  answered with a cancel. QS/400 has no inquiry here and drops the column.
- CHECK conditions: a special register or a subquery is SQL0546 / 42621;
  SYSCHKCST.CHECK_CLAUSE is the condition's tokens with a blank after each
  when made by SQL, the text as given when made by ADDPFCST.
- Identity: INCREMENT BY 0 is accepted (every value the same); MINVALUE
  above MAXVALUE, a CHAR or DECIMAL(5,2) identity or sequence: SQL0846 /
  42815; two identities: SQL0372 / 428C1; RESTART or DROP IDENTITY on an
  ordinary column: SQL0190 / 42837; ADD COLUMN ... AS IDENTITY on a table
  with rows numbers them 1, 2, 3; IDENTITY_VAL_LOCAL() is DECIMAL(31,0).
  An ALTER of the increment without RESTART goes on after the 20 values
  the column had cached (after RESTART WITH 1 and one insert, INCREMENT BY
  10 gives 30 next).
- Sequences: a sequence is a *DTAARA (SQL0601 names type *DTAARA);
  PREVIOUS VALUE before NEXT VALUE: SQL0845 / 51035; past MAXVALUE without
  CYCLE: SQL0359 / 23522; NO MINVALUE of an ascending sequence is its START
  WITH; SYSSEQUENCES.MAXASSIGNEDVAL is the end of the cached block.
- ROW CHANGE TIMESTAMP on a DATE: SQL0683 / 42842. GENERATED ALWAYS AS
  (K + 1): SQL0104 (expects IDENTITY). IMPLICITLY HIDDEN is accepted.
