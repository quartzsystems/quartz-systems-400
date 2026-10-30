# DDM and DRDA

`net/ddm` is QS/400's Distributed Data Management and Distributed
Relational Database Architecture: the datastream other DB2 systems,
IBM i systems and JTOpen speak on port 446, the application server
(`ddm::server::Session`) and requester (`ddm::client::Connection`) as
library code, and the objects behind `ADDRDBDIRE`, `CRTDDMF` and
`CHGDDMTCPA`. The TCP listener (`STRTCPSVR *DDM`, jobs `QRWTLSTN` and
`QRWTSRVR`) is the TCP server framework's; it reads chains with
`wire::read_chain`, hands them to `Session::handle` and writes back what
comes out.

Everything here was checked against pub400 (IBM i 7.5) through the
recording proxy (`tn5250-driver ddm-trace`); the traces and what they
showed are in `tools/conformance/ddm/traces/README.md`.
`net/ddm/tests/traces.rs` decodes and re-encodes them byte for byte,
`tests/session.rs` runs the requester against the server over the real
database in memory, and `tests/spec.rs` holds the tables below against
the code.

## Datastream

A connection carries chains of DSSes. A DSS is a six-byte header and
objects:

<!-- layout: dss::header -->

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | `length` |
| 2 | 1 | `magic` |
| 3 | 1 | `format` |
| 4 | 2 | `correlation` |

`magic` is `0xD0`. The format byte's low nibble is the type (1 request,
2 reply, 3 object, 4 encrypted object, 5 request without reply), `0x40`
chains the next DSS, `0x10` says it has the same correlation ID, `0x20`
continues the chain after an error. A DSS longer than 32,767 bytes is
segmented: the length's high bit says a continuation follows, each
continuation has a two-byte length with the same bit. A request DSS
holds one command; its data (`SQLSTT`, `SQLDTA`, `S38BUF`) follows in
object DSSes with the same correlation ID; the replies to it carry
that ID too. pub400 sends `EXCSATRD` and `ACCSECRD` as object DSSes.

An object is a two-byte length (header included), a two-byte code point
and its data. A length with the high bit on is extended: its low bits
less four count the bytes after the code point that hold the data's
length. Commands, reply messages, `EXCSATRD`, `ACCSECRD`, `TYPDEFOVR`
and `SQLDTA` hold further objects; everything else is scalar.
`ddm::wire::Object` keeps the tree and re-encodes it as it came.

## Getting in

1. `EXCSAT` names both sides (`EXTNAM`, `SRVNAM`, `SRVCLSNM`,
   `SRVRLSLV`) and the managers with their levels (`MGRLVLLS`); the
   reply grants the lower of each. QS/400 asks for SQLAM 7; pub400
   grants it. QS/400 calls itself `QS400/QSD` with product ID
   `QSD01000`: a requester saying `QAS` or `QSQ...` gets IBM i's private
   SQLCA objects (`0xD40A`, `0xD412`, `0xD413`) back instead of DRDA's.
2. `ACCSEC` asks for a security mechanism, with a token for the ones
   that need it. When the server does not take it, `ACCSECRD` lists the
   ones it does: pub400 offers 8, 7, 9 and 10.
3. `SECCHK` proves the user; `SECCHKRM` answers `SECCHKCD` 0 or the
   reason. A failed check ends the chain: the `ACCRDB` after it gets no
   reply.
4. `ACCRDB` opens the RDB with the requester's data representation
   (`TYPDEFNAM QTDSQL400`, `TYPDEFOVR CCSIDSBC 37`); `ACCRDBRM` gives the
   server's. Record-level access needs no `ACCRDB`.

## Security mechanisms

| `SECMEC` | Mechanism | Tokens | QS/400 |
|---|---|---|---|
| 3 | `USRIDPWD` | none; the password in the clear (CCSID 500) | requester and server; not offered by default |
| 6 | `USRSBSPWD` | eight-byte seeds both ways | DES substitute (password levels 0, 1) |
| 7 | `USRENCPWD` | DH public keys both ways | user ID clear, password DES-encrypted |
| 8 | `USRSSBPWD` | eight-byte seeds both ways | SHA-1 substitute (levels 2, 3); SHA-512 for level 4 |
| 9 | `EUSRIDPWD` | DH public keys both ways | user ID and password DES-encrypted |

The substitutes are the host servers' (`hostsvr::crypt`), over the
requester's seed from `ACCSEC` and the server's from `ACCSECRD`; the
proxy checked JTOpen's against ours on pub400. The Diffie-Hellman
exchange is 256 bits with DRDA's prime and generator
(`ddm::sec::DES_PRIME`, `DES_GENERATOR`). The DES key is bytes 12 to 19
of the shared secret; the CBC vector is the first eight bytes of the
user ID (`USRENCPWD`) or bytes 12 to 19 of the server's public key
(`EUSRIDPWD`); padding is PKCS #5. The user ID is upper case EBCDIC
blank padded to ten bytes, the password as typed in CCSID 500. All
three worked against pub400. The 512-bit AES form (`ENCALG *AES`) and
`EUSRIDNWPWD` (10) are not built.

`CHGDDMTCPA PWDRQD` picks what the server offers: `*USRENCPWD` (the
default) offers 8, 7 and 9; `*ENCUSRPWD` only 9; the rest add 6 and 3.
`*USRID`, `*VLDONLY` and `*KERBEROS` are accepted and stored; mechanisms
that prove no password and Kerberos are not built.

## SQL

Dynamic SQL needs a package: each statement is named by `PKGNAMCSN`
(RDB, collection, package, consistency token, section).

<!-- layout: pkg::pkgnamcsn -->

| Offset | Size | Field |
|---|---|---|
| 0 | 18 | `rdbnam` |
| 18 | 18 | `rdbcolid` |
| 36 | 18 | `pkgid` |
| 54 | 8 | `pkgcnstkn` |
| 62 | 2 | `pkgsn` |

Names longer than 18 use the extended form, each with a two-byte
length. On IBM i the package must exist: `NULLID/SYSSH200` is there
but not for an ordinary profile (-551), a missing one is -805. QS/400's
requester binds one (`Connection::bind_dynamic`: `BGNBND`, a
`BNDSQLSTT` of `DECLARE Cn CURSOR FOR Sn` per section, `ENDBND`); its
server takes any package and section, as packages hold nothing yet.

- `PRPSQLSTT` with `SQLSTT` (a null mixed string, then the single-byte
  string with a four-byte length) prepares; `RTNSQLDA` asks for the
  `SQLDARD`. A failed prepare answers `SQLCARD` alone.
- `OPNQRY` answers `OPNQRYRM` (limited block protocol `0x2417`, a
  `QRYINSID`) and `QRYDSC`; a short answer set may come in the same
  reply. `CNTQRY` gives `QRYDTA`; the last block ends with the `+100`
  row, `ENDQRYRM` and an `SQLCARD`. `CLSQRY` needs the `QRYINSID`.
- `EXCSQLIMM` with `SQLSTT` and `EXCSQLSTT` answer `SQLCARD`.
- `RDBCMM` and `RDBRLLBCK` answer `ENDUOWRM` (`UOWDSP` 1, 2) and
  `SQLCARD`.

The SQLCA group (`SQLCARD`, and the first group of `SQLDARD` and of the
end row) is a null indicator, `SQLCODE`, `SQLSTATE`, `SQLERRPROC`, the
nullable `SQLCAXGRP` (`SQLERRD1`-`6`, `SQLWARN0`-`A`, `SQLRDBNAME`, the
message tokens mixed and single-byte, separated by `0xFF`) and from
SQLAM 7 the nullable `SQLDIAGGRP`. `SQLDARD` adds the nullable
statement header, the column count and per column precision, scale,
an eight-byte length, `SQLTYPE`, CCSID and the optional names, labels
and comments.

`QRYDSC` is FD:OCA triplets. IBM i describes a character column (dates,
times and timestamps among them) by an MDD (`07 78 00 05 01 01 T`, `T`
the DRDA type) and an SDA (`0C 70 ID` with the FD:OCA type, CCSID,
character size, mode and 32,767) and lists the columns in the GDA
(`76 D0`) as the SDA's ID and the length; a number appears in the GDA
by its DRDA type itself (an early descriptor). The row and set
descriptors follow (`E0`, `F0`). A `QRYDTA` row is the null SQLCA, the
group indicator and the values, a nullable value after its own
indicator. QS/400 writes the same form. Numbers are big-endian
(`QTDSQL400`); decimals packed, numerics zoned, floats IEEE.

## Record-level access

IBM i systems and JTOpen do record access with the System/38
extension commands, each naming the open file by `DCLNAM`. They are
the DDM file model's open, get, insert, update, delete and close; the
level 3 file commands of the DDM architecture (`OPEN`, `GETREC`,
`INSRECEF` ...) are not built: no IBM i peer sends them and no trace
holds them.

| Command | Data | Reply |
|---|---|---|
| `S38OPEN` | `S38UFCB` | `S38OPNFB`, `S38IOFB` |
| `S38GET`, `S38GETM`, `S38GETK` | `S38OPTL`, `S38CTLL` | `S38BUF` and `S38IOFB`, or `S38IOFB` and `S38MSGRM` `CPF5001` (end) / `CPF5006` (no such key) |
| `S38PUTM` | `S38BUF` | `S38IOFB` |
| `S38UPDAT` | `S38OPTL`, `S38BUF` | `S38IOFB` |
| `S38DEL` | | `S38IOFB` |
| `S38FEOD` | `S38OPTL` | `S38IOFB`, `S38MSGRM` |
| `S38CLOSE` | `S38CLOST` | `S38MSGRM` |
| `S38CMD` | `S38CMDST` | `S38MSGRM` per message |

The operation list is the operation (`0x01` first, `0x03` next, `0x0B`
key equal, `0x21` update), the lock (`0x03` for update) and two option
bytes. The control list is entries of an ID, a two-byte length and the
data, ended by `0xFF`: `0x01` the record format, `0x07` the key bytes
(in the record's encoding), `0x08` the number of key fields, `0x10` the
records wanted. Each record in `S38BUF` is followed by eleven bytes (a
flag word, the relative record number, three reserved bytes, `0x40`
when read for update, a reserved byte) and the null field map, one
EBCDIC `0` or `1` per field.

<!-- layout: ufcb::fixed -->

| Offset | Size | Field |
|---|---|---|
| 0 | 10 | `file` |
| 10 | 2 | `library_id` |
| 12 | 10 | `library` |
| 22 | 2 | `member_id` |
| 24 | 10 | `member` |
| 46 | 2 | `open_flags` |
| 48 | 4 | `release` |
| 80 | 2 | `entry_marker` |

The open flags are `0x1002` with `0x20` input, `0x10` output, `0x08`
update and `0x04` delete. After the fixed part (83 bytes) come
`0x003C` with `0x80` for arrival sequence (absent for output only),
`0x003A` with `0xC0` (`0x40` when the open updates or deletes), the
blocking factor, `0x0009 0001 0001` and the record format, and
`0x7FFF`.

<!-- layout: feedback::opnfb -->

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | `open_flags` |
| 1 | 10 | `file` |
| 11 | 10 | `library` |
| 21 | 10 | `member` |
| 31 | 2 | `record_length` |
| 35 | 4 | `input_buffer_length` |
| 39 | 4 | `output_buffer_length` |
| 43 | 4 | `records` |
| 47 | 2 | `access_type` |
| 49 | 1 | `duplicate_keys` |
| 61 | 2 | `records_per_transfer` |
| 63 | 2 | `record_increment` |
| 88 | 2 | `ccsid` |

<!-- layout: feedback::iofb -->

| Offset | Size | Field |
|---|---|---|
| 0 | 10 | `record_format` |
| 10 | 4 | `record_length` |
| 14 | 2 | `records_processed` |
| 55 | 4 | `relative_record_number` |

## The server and requester

`Session::new(Config, Authenticator, SqlBackend, FileBackend)`;
`Session::handle(&[Dss]) -> Vec<Dss>` answers one chain. `Config` has
the local RDB name, the system name, the job name for `EXTNAM`, the
mechanisms offered and the rows per block. The traits:

- `SqlBackend`: `prepare`, `describe`, `execute`, `execute_immediate`,
  `open`, `fetch`, `close`, `commit`, `rollback`, each statement named by
  its `Section` (collection, package, number); errors are the `Sqlca`.
- `FileBackend`: `open` (a `FileOpen` of file, library, member, the
  four uses and keyed or arrival) giving a handle and `FileInfo`,
  `read` (`ReadAt::First`, `Next`, `Key`, `Rrn`, with or without a lock),
  `write`, `update`, `delete`, `feod`, `close`, `command`; errors are an
  IBM message ID and Quartz text.
- `Authenticator::check(user, Proof)`: `Proof::Password` or
  `Proof::Substitute` with both seeds; the answer is a `SECCHKCD`.

`Connection::connect(stream, &ConnectOpts)` runs the four steps (or
three, `access_rdb: false`), trying the mechanisms in order and taking
the first the server offers. Then `prepare`, `query` (open and
continue to the end), `close`, `execute_immediate`, `execute`,
`bind_dynamic`, `commit`, `rollback`; and for DDM files `open_file`,
`get`, `get_key`, `put`, `update_rec`, `delete_rec`, `close_file`,
`remote_command`.

## Objects and commands

The relational database directory, DDM files and the server's
attributes are `ddm::objects` payloads encoded as `KEYWORD(value)`
text. The commands run in the job (`work/qsjob/src/command/ddm.rs`):
`ADDRDBDIRE`, `CHGRDBDIRE`, `RMVRDBDIRE` (a name, a generic name,
`*ALL`, `*ALLRMT`), `WRKRDBDIRE` (the list display of
`objects::panel`, option 5 its detail), `CRTDDMF`, `CHGDDMF` and
`CHGDDMTCPA`. A directory never written holds the `*LOCAL` entry for
the system name, which is how install creates it.

Deviations, each marked `BLOCKED(phase10)` where it lives:

- The directory is kept in the data area `QUSRSYS/QRWRDBDIR`, a
  library's DDM files in its data area `QRWDDMF`, the attributes in
  `QUSRSYS/QRWDDMTCPA`; 2,000 characters hold about a dozen entries.
  DDM files belong in `*FILE` objects of attribute `DDM`, which the
  database's file kinds do not have yet, so opening one does not yet
  reach the remote file.
- The directory's message IDs are generic (`CPF9801`, `CPF9870`); the
  IBM IDs for these refusals are an open question.
- WRKRDBDIRE's options other than 5 and its print output are not
  built; the command shows the list as text.

## Tables

<!-- consts: cp::cmd -->

| Value | Name |
|---|---|
| `0x1041` | `EXCSAT` |
| `0x106D` | `ACCSEC` |
| `0x106E` | `SECCHK` |
| `0x2001` | `ACCRDB` |
| `0x2002` | `BGNBND` |
| `0x2004` | `BNDSQLSTT` |
| `0x2005` | `CLSQRY` |
| `0x2006` | `CNTQRY` |
| `0x2007` | `DRPPKG` |
| `0x2008` | `DSCSQLSTT` |
| `0x2009` | `ENDBND` |
| `0x200A` | `EXCSQLIMM` |
| `0x200B` | `EXCSQLSTT` |
| `0x200C` | `OPNQRY` |
| `0x200D` | `PRPSQLSTT` |
| `0x200E` | `RDBCMM` |
| `0x200F` | `RDBRLLBCK` |
| `0x2010` | `REBIND` |
| `0x2012` | `DSCRDBTBL` |
| `0x2014` | `EXCSQLSET` |
| `0x105A` | `CMMUOW` |
| `0x105B` | `RLLBCKUOW` |
| `0x105C` | `STRCMMCTL` |
| `0x102C` | `DCLFIL` |

<!-- consts: cp::mgr -->

| Value | Name |
|---|---|
| `0x1403` | `AGENT` |
| `0x1405` | `CMBACCAM` |
| `0x1406` | `CMBKEYAM` |
| `0x1407` | `CMBRNBAM` |
| `0x140C` | `DIRFIL` |
| `0x1419` | `DRCAM` |
| `0x141E` | `KEYFIL` |
| `0x1422` | `LCKMGR` |
| `0x1423` | `ALTINDF` |
| `0x1432` | `RELKEYAM` |
| `0x1433` | `RELRNBAM` |
| `0x1434` | `RNDKEYAM` |
| `0x1435` | `RNDRNBAM` |
| `0x143B` | `SEQFIL` |
| `0x143C` | `SUPERVISOR` |
| `0x1440` | `SECMGR` |
| `0x1444` | `CMNAPPC` |
| `0x1457` | `DIRECTORY` |
| `0x1458` | `DICTIONARY` |
| `0x1463` | `STRAM` |
| `0x1465` | `STRFIL` |
| `0x1472` | `FILISOLVL` |
| `0x1474` | `CMNTCPIP` |
| `0x147F` | `SYSCMDMGR` |
| `0x14A0` | `RSCRCVM` |
| `0x14C0` | `SYNCPTMGR` |
| `0x14C1` | `RSYNCMGR` |
| `0x14CC` | `CCSIDMGR` |
| `0x1C01` | `XAMGR` |
| `0x1C08` | `UNICODEMGR` |
| `0x2407` | `SQLAM` |
| `0x240F` | `RDB` |

<!-- consts: cp::obj -->

| Value | Name |
|---|---|
| `0x002F` | `TYPDEFNAM` |
| `0x0035` | `TYPDEFOVR` |
| `0x0010` | `FDODSC` |
| `0x146C` | `EXTDTA` |
| `0x147A` | `FDODTA` |
| `0x2408` | `SQLCARD` |
| `0x240B` | `SQLCINRD` |
| `0x240E` | `SQLRSLRD` |
| `0x2411` | `SQLDARD` |
| `0x2412` | `SQLDTA` |
| `0x2413` | `SQLDTARD` |
| `0x2414` | `SQLSTT` |
| `0x2419` | `SQLSTTVRB` |
| `0x241A` | `QRYDSC` |
| `0x241B` | `QRYDTA` |
| `0x2450` | `SQLATTR` |

<!-- consts: cp::param -->

| Value | Name |
|---|---|
| `0x000C` | `CODPNT` |
| `0x110E` | `FILNAM` |
| `0x112E` | `PRDID` |
| `0x1136` | `DCLNAM` |
| `0x113F` | `PRCCNVCD` |
| `0x1143` | `RLSFILLK` |
| `0x1145` | `RQSFILLK` |
| `0x1147` | `SRVCLSNM` |
| `0x1149` | `SVRCOD` |
| `0x114A` | `SYNERRCD` |
| `0x1153` | `SRVDGN` |
| `0x1155` | `RTNINA` |
| `0x115A` | `SRVRLSLV` |
| `0x115D` | `SPVNAM` |
| `0x115E` | `EXTNAM` |
| `0x1165` | `DRCNAM` |
| `0x116D` | `SRVNAM` |
| `0x1196` | `SECMGRNM` |
| `0x119C` | `CCSIDSBC` |
| `0x119D` | `CCSIDDBC` |
| `0x119E` | `CCSIDMBC` |
| `0x11A0` | `USRID` |
| `0x11A1` | `PASSWORD` |
| `0x11A2` | `SECMEC` |
| `0x11A4` | `SECCHKCD` |
| `0x11B5` | `LUWHLDCSR` |
| `0x11BA` | `CMMCTLTYP` |
| `0x11DC` | `SECTKN` |
| `0x11DE` | `NEWPASSWORD` |
| `0x1404` | `MGRLVLLS` |
| `0x1909` | `ENCALG` |
| `0x190A` | `ENCKEYLEN` |
| `0x1900` | `MONITOR` |
| `0x2102` | `QRYPRCTYP` |
| `0x2103` | `RDBINTTKN` |
| `0x2104` | `PRDDTA` |
| `0x2105` | `RDBCMTOK` |
| `0x2108` | `RDBCOLID` |
| `0x2109` | `PKGID` |
| `0x210A` | `PKGNAM` |
| `0x210C` | `PKGSN` |
| `0x210D` | `PKGCNSTKN` |
| `0x210E` | `RTNSETSTT` |
| `0x210F` | `RDBACCCL` |
| `0x2110` | `RDBNAM` |
| `0x2111` | `OUTEXP` |
| `0x2112` | `PKGNAMCT` |
| `0x2113` | `PKGNAMCSN` |
| `0x2114` | `QRYBLKSZ` |
| `0x2115` | `UOWDSP` |
| `0x2116` | `RTNSQLDA` |
| `0x2118` | `STTSTRDEL` |
| `0x2119` | `STTDECDEL` |
| `0x211A` | `RDBALWUPD` |
| `0x211F` | `SQLCSRHLD` |
| `0x2125` | `PKGDFTCST` |
| `0x2128` | `DFTRDBCOL` |
| `0x2132` | `QRYBLKCTL` |
| `0x2135` | `CRRTKN` |
| `0x213A` | `NBRROW` |
| `0x213D` | `QRYROWNBR` |
| `0x2140` | `MAXRSLCNT` |
| `0x2141` | `MAXBLKEXT` |
| `0x2142` | `RSLSETFLG` |
| `0x2146` | `TYPSQLDA` |
| `0x2147` | `OUTOVROPT` |
| `0x2148` | `RTNEXTDTA` |
| `0x2149` | `QRYATTSCR` |
| `0x214A` | `QRYRELSCR` |
| `0x2150` | `QRYATTUPD` |
| `0x2152` | `QRYSCRORN` |
| `0x2153` | `QRYROWSNS` |
| `0x2154` | `QRYBLKRST` |
| `0x2155` | `QRYRTNDTA` |
| `0x2156` | `QRYROWSET` |
| `0x2157` | `QRYATTSNS` |
| `0x215B` | `QRYINSID` |
| `0x215D` | `QRYCLSIMP` |
| `0x215E` | `QRYCLSRLS` |
| `0xD142` | `SXXFACTOR` |
| `0xD143` | `SXXVERID` |
| `0xD144` | `SXXCLTIP` |
| `0xD11F` | `S38UFCB` |
| `0xD119` | `S38OPTL` |
| `0xD105` | `S38CTLL` |
| `0xD405` | `S38BUF` |
| `0xD402` | `S38IOFB` |
| `0xD404` | `S38OPNFB` |
| `0xD406` | `S38LCNRD` |
| `0xD121` | `S38CLOST` |
| `0xD103` | `S38CMDST` |
| `0xD10E` | `S38MDATA` |
| `0xD111` | `S38MFILE` |
| `0xD112` | `S38MID` |
| `0xD116` | `S38MTEXT` |
| `0xD117` | `S38MTYPE` |
| `0xD10B` | `S38LRQS` |
| `0xD10C` | `S38LWAIT` |
| `0xD115` | `S38LRLS` |

<!-- consts: cp::rm -->

| Value | Name |
|---|---|
| `0x1219` | `SECCHKRM` |
| `0x2201` | `ACCRDBRM` |
| `0x2202` | `QRYNOPRM` |
| `0x2203` | `RDBATHRM` |
| `0x2204` | `RDBNACRM` |
| `0x2205` | `OPNQRYRM` |
| `0x2207` | `RDBACCRM` |
| `0x220B` | `ENDQRYRM` |
| `0x220C` | `ENDUOWRM` |
| `0x220D` | `ABNUOWRM` |
| `0x220E` | `DTAMCHRM` |
| `0x220F` | `QRYPOPRM` |
| `0x2211` | `RDBNFNRM` |
| `0x2212` | `OPNQFLRM` |
| `0x2213` | `SQLERRRM` |
| `0x2218` | `RDBUPDRM` |
| `0x221A` | `RDBAFLRM` |
| `0x1210` | `MGRLVLRM` |
| `0x1218` | `MGRDEPRM` |
| `0x121C` | `CMDATHRM` |
| `0x1232` | `AGNPRMRM` |
| `0x1233` | `RSCLMTRM` |
| `0x1245` | `PRCCNVRM` |
| `0x124B` | `CMDCMPRM` |
| `0x124C` | `SYNTAXRM` |
| `0x1250` | `CMDNSPRM` |
| `0x1251` | `PRMNSPRM` |
| `0x1252` | `VALNSPRM` |
| `0x1253` | `OBJNSPRM` |
| `0x1254` | `CMDCHKRM` |
| `0x125F` | `TRGNSPRM` |
| `0xD201` | `S38MSGRM` |

<!-- consts: cp::rpy -->

| Value | Name |
|---|---|
| `0x1443` | `EXCSATRD` |
| `0x14AC` | `ACCSECRD` |

<!-- consts: cp::s38 -->

| Value | Name |
|---|---|
| `0xD002` | `S38ALCOB` |
| `0xD004` | `S38CLOSE` |
| `0xD006` | `S38CMD` |
| `0xD007` | `S38DEL` |
| `0xD008` | `S38DLCOB` |
| `0xD00B` | `S38FEOD` |
| `0xD00C` | `S38GET` |
| `0xD00D` | `S38GETD` |
| `0xD00E` | `S38GETK` |
| `0xD00F` | `S38GETM` |
| `0xD011` | `S38OPEN` |
| `0xD012` | `S38PUT` |
| `0xD013` | `S38PUTM` |
| `0xD019` | `S38UPDAT` |
| `0xD01C` | `SXXPUTDR` |
| `0xD02A` | `SXXASPRQ` |

<!-- consts: cp::secchkcd -->

| Value | Name |
|---|---|
| `0x0000` | `OK` |
| `0x0001` | `SECMEC_NOT_SUPPORTED` |
| `0x000B` | `SECTKN_MISSING` |
| `0x000E` | `PASSWORD_EXPIRED` |
| `0x000F` | `PASSWORD_INVALID` |
| `0x0010` | `PASSWORD_MISSING` |
| `0x0012` | `USERID_MISSING` |
| `0x0013` | `USERID_INVALID` |
| `0x0014` | `USERID_REVOKED` |
| `0x0015` | `NEWPASSWORD_INVALID` |
| `0x001B` | `ENCALG_NOT_SUPPORTED` |

<!-- consts: cp::secmec -->

| Value | Name |
|---|---|
| `0x0001` | `DCESEC` |
| `0x0003` | `USRIDPWD` |
| `0x0004` | `USRIDONL` |
| `0x0005` | `USRIDNWPWD` |
| `0x0006` | `USRSBSPWD` |
| `0x0007` | `USRENCPWD` |
| `0x0008` | `USRSSBPWD` |
| `0x0009` | `EUSRIDPWD` |
| `0x000A` | `EUSRIDNWPWD` |
| `0x000B` | `KERSEC` |
| `0x000C` | `EUSRIDDTA` |
| `0x000D` | `EUSRPWDDTA` |
| `0x0010` | `EUSRIDONL` |

<!-- consts: cp::svrcod -->

| Value | Name |
|---|---|
| `0x0000` | `INFO` |
| `0x0004` | `WARNING` |
| `0x0008` | `ERROR` |
| `0x0010` | `SEVERE` |
| `0x0020` | `ACCDMG` |
| `0x0040` | `PRMDMG` |
| `0x0080` | `SESDMG` |

<!-- consts: desc::fd -->

| Value | Name |
|---|---|
| `0x0001` | `FIXEDBYTES` |
| `0x0002` | `VARBYTES` |
| `0x0010` | `FIXEDCHAR` |
| `0x0011` | `VARCHAR` |
| `0x0019` | `VARMIX` |
| `0x0023` | `INTEGER_BE` |
| `0x0024` | `INTEGER_LE` |
| `0x0030` | `DECIMAL` |
| `0x0033` | `ZONED` |
| `0x0048` | `FLOAT_IEEE` |

<!-- consts: types::drda -->

| Value | Name |
|---|---|
| `0x0002` | `INTEGER` |
| `0x0004` | `SMALL` |
| `0x000A` | `FLOAT8` |
| `0x000C` | `FLOAT4` |
| `0x000E` | `DECIMAL` |
| `0x0010` | `ZDECIMAL` |
| `0x0016` | `INTEGER8` |
| `0x0020` | `DATE` |
| `0x0022` | `TIME` |
| `0x0024` | `TIMESTAMP` |
| `0x0026` | `FIXBYTE` |
| `0x0028` | `VARBYTE` |
| `0x0030` | `CHAR` |
| `0x0032` | `VARCHAR` |
| `0x0034` | `LONG` |
| `0x003C` | `MIX` |
| `0x003E` | `VARMIX` |
| `0x0040` | `LONGMIX` |

<!-- consts: types::sqltype -->

| Value | Name |
|---|---|
| `0x0180` | `DATE` |
| `0x0184` | `TIME` |
| `0x0188` | `TIMESTAMP` |
| `0x01C0` | `VARCHAR` |
| `0x01C4` | `CHAR` |
| `0x01C8` | `LONGVARCHAR` |
| `0x01E0` | `FLOAT` |
| `0x01E4` | `DECIMAL` |
| `0x01E8` | `NUMERIC` |
| `0x01EC` | `BIGINT` |
| `0x01F0` | `INTEGER` |
| `0x01F4` | `SMALLINT` |
