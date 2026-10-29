# JTOpen host server suite

Small Java programs that drive the host servers through JTOpen, the
IBM Toolbox for Java, the way client applications do. They run on the
harness host, never in the image, against QS/400 or the oracle, through
the recording proxy of `tn5250-driver hostsvr-trace`, which writes the
wire traces under `tools/conformance/hostsvr/traces/`. Protocol notes
are in `docs/HOSTSVR.md`.

JTOpen is IBM's open source Toolbox for Java, licensed under the IBM
Public License 1.0. It is a test client only: these programs link
against `jt400.jar` at run time, nothing from it is copied into
QS/400, and its source was read only to understand the datastream.

## Programs

| Scenario | Class | What it exercises |
|---|---|---|
| `signon` | `Signon` | Sign-on server: validate, release, sign-on information, a profile token, an unknown user |
| `pwdlevel` | `PwdLevel` | Starts each server and reports the password level JTOpen learned |
| `svrmap` | `Svrmap` | Every service looked up through the port mapper (449), then started |
| `cmdcall` | `CmdCall` | `CommandCall`: commands that succeed and fail, with their messages |
| `pgmcall` | `PgmCall` | `ProgramCall` of `QWCRSVAL` and `QUSRJOBI` with marshalled parameters; a missing program |
| `sysval` | `SysVal` | `SystemValue` |
| `central` | `Central` | Central server: the job CCSID and conversion table downloads |
| `jdbc` | `Jdbc` | `jdbc:as400://`: connect, metadata, blocked fetch over `QSYS2.SYSTABLES`, a table made in `QSTEST` (`QTEMP` when `QSTEST` does not exist) and dropped, an SQL error, commit and rollback, a procedure call |
| `dtaq` | `DtaQ` | `DataQueue` and `KeyedDataQueue` in `QSTEST`, deleted after |
| `ifs` | `Ifs` | `IFSFile` and streams under `/home/QSTEST` only, removed after |
| `splf` | `Splf` | `SpooledFileList` of the user's spooled files (list only) |

Each program prints one `ok NAME` or `fail NAME: ...` line per step,
with indented result lines; the user's name is printed as `{user}`.
The oracle's output is kept in `expected/oracle/<scenario>.out`.

## Running

From the repository root, with a JDK on the path and the oracle
credentials in `tools/conformance/oracle/.env` (or `QS_ORACLE_ENV`
pointing at it):

```
JT400_JAR=/path/to/jt400.jar tools/conformance/jtopen/run.sh [scenario...]
```

`run.sh` compiles the programs into `target/jtopen/classes`, builds the
driver, and runs each scenario (all of them by default) as

```
tn5250-driver hostsvr-trace --target oracle --out tools/conformance/hostsvr/traces \
  --scenario NAME -- java -cp CLASSES:JAR CLASS
```

The proxy listens on 127.0.0.1 at 449 and 8470-8476 and hands the
program `QS_HS_HOST=127.0.0.1`, `QS_HS_USER` and `QS_HS_PASS`. It
pauses `QS_HS_PAUSE` seconds (20) between scenarios: the oracle drops
new connections while it recycles the previous run's prestart jobs.

`QS_HS_SKIP_SIGNON=1` (the default in `run.sh`) makes every program but
`signon` and `pwdlevel` start its server directly, with the seed
exchange and start server on the server's own port, instead of signing
on through the sign-on server first: the primary oracle's sign-on and
central servers accept connections and close them without a reply.
Set `QS_HS_SKIP_SIGNON=0` against QS/400.

## Masking

Traces are masked as they are recorded, so no credential reaches the
disk. In every frame of the sign-on family (request IDs `0x70xx` and
`0x71xx`, replies `0xF0xx` and `0xF1xx`):

- the user ID (`0x1104`; in replies, after its 4-byte CCSID) becomes the
  EBCDIC placeholder `MASKUSER`, blank padded to the field's length;
- the password substitute (`0x1105`), the protected old and new
  passwords (`0x110C`, `0x110D`) and profile tokens (`0x1115`) become
  `0xFF` bytes of the same length.

Lengths never change, so masked traces still parse and re-encode
byte for byte. The proxy also checks each substitute it sees against
`hostsvr::crypt` before masking it and records the outcome as a `note`
line, and it writes nothing if the password itself shows up on the
wire in ASCII, EBCDIC or UTF-16. `net/hostsvr/tests/traces.rs` fails if
a committed trace has an unmasked field.
