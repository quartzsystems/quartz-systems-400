# DDM/DRDA traces

Byte traces of port 446 on pub400 (IBM i 7.5), recorded 2026-10-29 with
`tn5250-driver ddm-trace`: a proxy on 127.0.0.1 forwards each connection
unchanged and writes every DSS both ways as a hex dump (format in
`net/ddm/src/trace/mod.rs`). `net/ddm/tests/traces.rs` reads them back,
re-encodes every DSS byte for byte and decodes the SQL objects.

## Masking

In `SECCHK` the user ID is `MASKUSER`, and the password, its substitute
and the encrypted tokens are `0xFF` bytes. The account's name anywhere
else (the library `{user}B` in package names, open feedback and
command strings) is the same number of placeholder characters. Lengths
never change. The proxy refuses to write a trace in which a plain-text
form of the password remains. For the substitute mechanism the proxy
recomputes the substitute with `ddm::sec` from the seeds and notes only
whether it matched.

## Scenarios

| File | Client | What it does |
|---|---|---|
| `secmec-offer.trace` | `ddm::client` | `EXCSAT`, `ACCSEC` asking for `USRIDONL` (4): `ACCSECRD` lists what the server offers (8, 7, 9, 10) |
| `secmec-7.trace` | `ddm::client` | connect with `USRENCPWD` (DH, DES), `RDBCMM` |
| `secmec-8.trace` | `ddm::client` | connect with `USRSSBPWD` (SHA-1 substitute), `RDBCMM` |
| `drda-sql.trace` | `ddm::client` | connect with `EUSRIDPWD`; bind package `{user}B/QSDDMPKG` with eight dynamic sections (`BGNBND`, `BNDSQLSTT`, `ENDBND`); prepare, open, continue a catalog query and a query of every scalar type; an SQL error (-204); `EXCSQLIMM` of a temporary table and an insert into it; query it; `RDBRLLBCK`, `RDBCMM` |
| `rla-read.trace` | JTOpen `SequentialFile` | read-only record access to `QIWS/QCUSTCDT`: the record format (`DSPFFD` to `QTEMP` by `S38CMD`), `S38OPEN`, `S38GET`, `S38GETM`, `S38CLOSE` |
| `rla-write.trace` | JTOpen `KeyedFile` | creates `{user}B/QSDDMT` keyed, `S38PUTM` three records, `S38GETK`, `S38UPDAT`, `S38DEL`, `S38FEOD`, reads it back, `DLTF` |

Everything created on pub400 was removed in the same session: the
package was dropped (`DROP PACKAGE`) and the file deleted. The temporary
table lived in `QTEMP`.

## What the traces showed

- `EXCSATRD` and `ACCSECRD` come back as object DSSes (type 3) chained
  to each other; the server is `QAS` `V07R05M00` and names its job
  (`nnnnnn/QUSER/QRWTSRVR`) in `EXTNAM`. It grants SQLAM 7.
- A requester that says `SRVCLSNM QAS` or `PRDID QSQ...` is answered with
  IBM i's private SQLCA objects (`0xD40A`, `0xD412`, `0xD413`) instead of
  `SQLCARD`; QS/400 uses `QS400/QSD` and `QSD01000`.
- Dynamic SQL needs a bound package: `NULLID/SYSSH200` exists but an
  ordinary profile is not authorized to it (-551); a package in an
  unbound collection is not found (-805). Binding one with
  `DECLARE Cn CURSOR FOR Sn` statements gives sections for `PRPSQLSTT`.
- `QRYDSC` describes character columns (dates and times among them) by
  an MDD and SDA pair and numbers by DRDA's early descriptors (the GDA
  entry's ID is the DRDA type); `OPNQRY` answers `OPNQRYRM`
  (`QRYPRCTYP` limited block) and `QRYDSC`, the rows come with
  `CNTQRY` followed by `ENDQRYRM` and an `SQLCARD` of +100.
- `CLSQRY` without `QRYINSID` is a syntax error (`SYNERRCD` 0x0E).
- JTOpen's record access uses the System/38 commands and `USRSSBPWD`
  with the host servers' SHA-1 substitute.
