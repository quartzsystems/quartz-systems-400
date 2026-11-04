# Printing over the network

Phase 10 lets a writer print somewhere other than the capture device: a
LAN printer device (`CRTDEVPRT DEVCLS(*LAN) LANATTACH(*IP)`) or a remote
output queue (`CRTOUTQ RMTSYS() CNNTYPE(*IP)`, `STRRMTWTR`). The logic
is the `print` crate (`work/print`): the host print transform, the
sinks, and `deliver`, which joins them. It knows nothing of the store;
the writer job (`work/qsjob/src/work/writer.rs`) maps what the service
tells it onto `print`'s types.

## Flow

1. `STRPRTWTR DEV(lanprt)` or `STRRMTWTR OUTQ(lib/q)` submits the writer
   job to `QSPL` as before (Phase 5). The service's writer table marks a
   remote writer (`Type RMT` in `WRKWTR`).
2. The writer job asks the service where it prints (`wtr_target`): the
   capture device, a LAN printer (`RMTLOCNAME`, `PORT`, `SYSDRVPGM`,
   `SECURECNN`, `TRANSFORM`, `MFRTYPMDL`, the `*WSCST` payload, `CHRID`)
   or a remote queue (host, `RMTPRTQ`, `DESTTYPE`, `DESTOPT`, `SEPPAGE`,
   `TRANSFORM`, `MFRTYPMDL`, `*WSCST`, this system's name).
3. File by file (`wtr_next`), it transforms the SCS and sends it.
   Success is `wtr_printed` with no text and no PDF: a network printer
   leaves no capture. Failure is `wtr_failed`: the file is `RDY` again
   (or `HLD` when the far end refused the file itself), the writer shows
   `MSGW`, a message goes to `QSYSOPR`, and the writer tries the file
   again after 30 seconds. The next success clears `MSGW`.

## Sinks (`print::sink`)

```rust
pub trait Sink {
    fn send(&mut self, job: &PrintJob, doc: &Output) -> Result<Receipt, SinkError>;
}
pub trait Stream: Read + Write + Send { fn finish(&mut self) -> io::Result<()>; }
pub trait TlsConnector: Send + Sync {
    fn connect(&self, host: &str, tcp: TcpStream) -> io::Result<Box<dyn Stream>>;
}
```

`PrintJob` is the spooled file's attributes, `Output` the transformed
bytes and their format. `SinkError` has a kind (`Resolve`, `Refused`,
`Timeout`, `Connect`, `Io`, `Protocol`, `Rejected`, `Tls`) and a line of
detail; `kind.msgid()` is the message the writer sends, `retryable()`
whether the file waits or is held.

| `SYSDRVPGM` | Sink | Port | Notes |
|---|---|---|---|
| `*HPPJLDRV` (default) | raw | 9100 | each file one PJL job: UEL, `@PJL JOB NAME`, `SET COPIES`, `ENTER LANGUAGE`, data, UEL, `@PJL EOJ`, UEL |
| `*IBMPJLDRV` | raw | 9100 | the same |
| `*IBMSNMPDRV` | raw | 9100 | bare data; no SNMP status is read |
| `*NETSTNDRV` | raw | 9100 | bare data |
| `*IBMIPPDRV` (`*IPPDRV` accepted) | IPP | 631 | `Print-Job`, below |

Nothing is read back from a raw port but its close: no `@PJL INFO
STATUS`, no SNMP. A printer that is off is a connect failure.

**IPP** (`ipp.rs`): IPP/1.1 `Print-Job` (RFC 8010/8011) POSTed with
HTTP/1.1 (`Content-Length`, `Connection: close`) to the printer URI.
`RMTLOCNAME` is a bare host (URI `ipp://host:631/ipp/print`, or `PORT`)
or a whole `ipp://`, `ipps://`, `http://`, `https://` URI;
`SECURECNN(*YES)` forces TLS. Operation attributes: `attributes-charset`
`utf-8`, `attributes-natural-language` `en`, `printer-uri`,
`requesting-user-name` (the file's user), `job-name`
(`FILE.NNNNNN.N`), `document-format` (`application/vnd.hp-PCL`,
`text/plain`, `application/pdf`, `application/octet-stream`) and, for
text and PDF, `document-name`. Job attributes: `copies`, `sides`
(`one-sided`, `two-sided-long-edge`, `two-sided-short-edge` from
`DUPLEX`), `media` (the paper the layout chose). A status of 0x0400 or
more fails the send; 0x04xx holds the file. The reply's `job-id` is kept
in the receipt. Responses with `Content-Length`, chunked or neither are
read.

TLS goes through `TlsConnector`; `PlainConnector` passes TCP through
(tests). The writer job's connector (`PrinterTls`,
`work/qsjob/src/work/writer.rs`) checks the printer's chain and name
against the shipped roots and the trusted CAs of `*SYSTEM`
(docs/CERTS.md); a printer with a self-signed certificate needs it
imported as a CA.

**LPR** (`lpr/mod.rs`, RFC 1179): `02 queue LF`, then the data file
(`03 count SP dfAnnnhost LF`, bytes, a zero octet), then the control
file (`02 count SP cfAnnnhost LF`, bytes, a zero octet), every step
acknowledged with one zero octet. The data file goes first, as BSD
`lpr` sends it. `nnn` is `(job number * 10 + file number) mod 1000`.
The source port is not a reserved one (721-731).

`RMTPRTQ(*USER)` sends the file's user name as the queue,
`RMTPRTQ(*SYSTEM)` sends `QPRINT`, anything else goes as typed. The host
is `RMTSYS`, or `INTNETADR` with `RMTSYS(*INTNETADR)`.

### `DESTOPT`

| Word | What the client does |
|---|---|
| `XAIX` | the copies as repeated print commands in one control file (AIX/BSD multiple copies) |
| (no `XAIX`) | the whole job sent once per copy |
| `XAUTOQ` | recognised; an IBM i receiver picks the output queue from the queue name, which is already what is sent |
| `*NOWAIT` | the last acknowledgement (after the control file) is not waited for |
| `*USRDFNTXT` | recognised, no effect yet |
| others | kept, not sent |

### The control file

```
H<this system's name>
P<user>
J<FILE.NNNNNN.N>
C<class>                 when given
L<user>                  SEPPAGE(*YES): a banner page
Y...                     DESTTYPE(*OS400): the extension below
l<dfA name>              once, or once per copy with XAIX
U<dfA name>
N<file name>
```

Data is always sent with `l` (print as is): PCL, text and SCS alike.

### IBM-to-IBM: the `Y` extension

For `DESTTYPE(*OS400)` (and `*OS400V2`, and the spec's `*IBMI` accepted
as `*OS400`) the data file is the SCS untransformed and the control file
carries the spooled file's attributes, so a receiving QS/400 LPD
recreates the file as it was. Other LPDs ignore the unknown letter.
`print::sink::lpr::control::ControlFile::parse` and `.job()` read it
back; the LPD server uses them.

Each line is `Y`, then `KEY=value`. The first is `YQS400=1` (the
version); a file without it has no extension. Values escape `%`, bytes
below 0x20, 0x7F and every non-ASCII byte as `%XX` (the UTF-8 bytes).

| Key | Value | `*SPLF` attribute |
|---|---|---|
| `FILE` | name | spooled file name |
| `SPLNBR` | number | file number in the job |
| `JOB` | name | job name |
| `USER` | name | job user |
| `JOBNBR` | six digits | job number |
| `SYSTEM` | name | the sending system |
| `USRDTA` | text | `USRDTA` |
| `FORMTYPE` | text | `FORMTYPE` |
| `COPIES` | number | `COPIES` (copies travel here; one transfer) |
| `PAGES` | number | pages |
| `PAGELEN`, `PAGEWIDTH` | numbers | `PAGESIZE` |
| `LPI`, `CPI` | numbers | `LPI`, `CPI` |
| `OVRFLW` | number | `OVRFLW` |
| `DRAWER` | number | `DRAWER` |
| `DUPLEX` | `*NO`, `*YES`, `*TUMBLE` | `DUPLEX` |
| `OUTPTY` | number | `OUTPTY` |
| `FONT` | identifier or `*CPI` | `FONT` |
| `CHRID` | `gcgid cpgid`, `*DEVD` | `CHRID` |
| `PRTQLTY` | `*STD` ... | `PRTQLTY` |
| `ALIGN` | `*YES`, `*NO` | `ALIGN` |
| `DEVTYPE` | `*SCS` | the data's type |
| `PRTF` | `lib/name` | the printer file |
| `USRDFNDTA` | text | `USRDFNDTA` |
| `SAVE` | `*YES`, `*NO` | `SAVE` |

Unknown keys are ignored; missing ones take `PrintJob::default()`.
This is our own convention: IBM's own IBM-to-IBM LPR transfer is not
documented and was not traced.

## The host print transform (`print::transform`)

`Transform::select(TRANSFORM, MFRTYPMDL, wscst)`: `TRANSFORM(*NO)` is
the SCS as it is; `*WSCST` the customizing object; a LaserJet-class
model PCL; `*PDF` (ours) the capture device's PDF emitter; another
model its shipped `QSYS/QWP...` object when the writer read one
(`*WSCST` below); anything else plain text. An IBM i destination always
gets the SCS.

**Text**: every line from 1 to the last printed, trailing blanks off,
CR LF, a form feed per page. Characters outside ASCII take the code page
`CHRID` names (850, 819, 1252 through the `ccsid` tables, 437 by a
table here); `*DEVD` prints them as `?`.

**PCL5** (`pcl.rs`, `page.rs`). The capability table:

| `MFRTYPMDL` | PCL | dpi | duplex | trays |
|---|---|---|---|---|
| `*HPII`, `*HPIIP`, `*HPPLUS` | 4 | 300 | no | no |
| `*HPIID` | 4 | 300 | yes | yes |
| `*HPIII`, `*HPIIIP` | 5 | 300 | no | no |
| `*HPIIID`, `*HPIIISI` | 5 | 300 | yes | yes |
| `*HP4` (`*HPLJ4`), `*HP5`, `*HP5SI` | 5e | 600 | yes | yes |

The job: `ESC E`; `ESC &l#A` paper; `ESC &l#O` orientation; `ESC &l#X`
copies; `ESC &l#H` tray (`DRAWER` 1 upper, 2 lower, 3 large capacity,
more automatic) on models with trays; `ESC &l#S` duplex on models with
one; `ESC &l#D` lines per inch; `ESC &l0E` top margin; `ESC &l#F` text
length (`PAGESIZE` length); `ESC &a0L`; `ESC &s1C` (no wrap); `ESC (#`
the symbol set of `CHRID` (437 `10U`, 850 `12U`, 852 `17U`, 857 `9T`,
860 `8U`, 1252 `19U`, 819 `0N`, else `10U`); the font, Courier fixed at
the pitch (`ESC (s0p#h#v0s0b4099T`; PCL4 models typeface 3 at 10, 12 or
16.67). Then the lines, CR LF each, a form feed per page, `ESC E`.

The pitch is `FONT`'s when the identifier fixes one (011 10, 085 12,
222/223 15, 252 16.67, 290 20 ...), else `CPI`. The page goes on letter,
A4 or legal at its own pitch and density, portrait before landscape;
failing that, computer output reduction onto letter (pitch 12, 15,
16.67, 20; density 8, 9, 12) until it fits; then A3 or ledger; last
ledger landscape at 20 and 12, clipped. `OVRFLW` is honored where the
printer file put the page breaks; the transform keeps them.

Golden outputs: `work/print/tests/golden/` (bytes shown with `<ESC>`,
`<CR>`, `<LF>`, `<FF>`, `<xHH>`), checked by hand against the PCL5
command reference. **Unverified against IBM's transform**: Cody's box
has no remote output queue pointing at a harness LPD, so no
`TRANSFORM(*YES)` output was captured.

### `*WSCST`

The source is IBM's tag language (`print::wscst::tags`), as `RTVWSCST`
writes it on IBM i: `:WSCST DEVCLASS=TRANSFORM.` first, `:EWSCST.`
last, between them tags `:NAME` with `KEY=value` attributes, each tag
ending at a period; values are numbers, words or quoted strings, `'..'X`
hexadecimal, several strings in a row one value, `/* */` comments.
`CRTWSCST` compiles a member (default `QTXTSRC`, member `*WSCST` = the
object's name); the key form earlier releases wrote (`KEY = value`
lines) still compiles. Tags the transform has no use for are read and
left out. The ones it uses:

| Tag | Attributes | Used for |
|---|---|---|
| `INITPRT`, `RESETPRT` | `DATA` | sent first, last |
| `CARRTN`, `LINEFEED`, `FORMFEED` | `DATA` | line and page ends |
| `PAGSIZE` | `PAGWTH`, `PAGLEN` (1440ths of an inch), `DATA` | a paper |
| `PRTORIENT` | `ORIENT=PORTRAIT\|LANDSCAPE`, `DATA` | orientation |
| `DWRSLT` | `DRAWER=DRAWERn`, `DATA` | input tray |
| `SMPXPRT`, `DUPXPRT`, `TUMDUPXPRT` | `DATA` | sides |
| `VARLSPC` | `VAROFFSET`, `VARLEN`, `VARTYPE=CHRDEC`, `CNVNUM`, `CNVDEN`, `DATA` | line density: the spacing in `CNVNUM/CNVDEN` inch put into `DATA` |
| `CPI` | `CPI=n`, `DATA` | pitch (ours) |
| `COPIES` | as `VARLSPC`, the count put in | copies (ours) |

The tag names and attributes of the first eight rows are the ones the
primary's `RTVWSCST DEVTYPE(*TRANSFORM) MFRTYPMDL(*HP4)` writes
(captured 2026-11-03, not kept in the tree); `CPI` and `COPIES` are
QS/400's own, since IBM's sources choose fonts by font tables QS/400
does not read. `RTVWSCST DEVTYPE(*TRANSFORM)` writes a LaserJet-class
model's PCL commands, another model's shipped object's source, and
`DEVTYPE(*CTXFORM)` an empty content transform source; the display
station device types are refused.

**Shipped objects.** `QSYS` holds the 153 `*WSCST` objects the
primary's `DSPOBJD QSYS/*ALL *WSCST` lists
(`tools/conformance/golden/phase12/oracle/wscst-qsys.txt`), made at
the first IPL (`lic/osobjects/src/wscst.rs`), attribute `TRANSFORM`
(`CTXFORM` for the three `QCTX...`), with Quartz content
(`print::wscst::shipped`): the LaserJet objects (`QWPHPLSR2`, `2D`, `3`,
`3D`, `4`, `5`, `QWPHP5SI`) the PCL transform's commands for their
model, the other PCL printers' (`QWPHP...`, `QWPLEX...`, `QWPEPLPCL4`,
`5`, `QWPCPQPM20`, `QWPPANLBP4`, `5`) the `*HP4` ones, every other
printer plain lines. Which object IBM's transform takes for a
LaserJet model is inferred from the names (`*HPII`, `*HPIIP`,
`*HPPLUS` `QWPHPLSR2`; `*HPIID` `QWPHPLSR2D`; `*HPIII`, `*HPIIIP`
`QWPHPLSR3`; `*HPIIID`, `*HPIIISI` `QWPHPLSR3D`; `*HP4` `QWPHPLSR4`;
`*HP5` `QWPHPLSR5`; `*HP5SI` `QWPHP5SI`); another `MFRTYPMDL` names
`QWP` and its value (`*IBM4019` is `QWPIBM4019`).

**The transform.** `MFRTYPMDL(*WSCST)` prints through the object
`WSCST()` names (device, remote output queue, `SNDTCPSPLF`); a missing
one ends `SNDTCPSPLF` with `CPF9801`. A LaserJet-class model keeps the
PCL transform (its own fit of the page, which the shipped object holds
the commands of); any other model that has a shipped object prints
through it (the writer reads `QSYS/QWP...`), and one without prints
plain text. `DLTWSCST` deletes objects by name or generic name as
`DLTOBJ` does (`*OBJEXIST`).

The `*WSCST` object (type 0x0028) holds `QSWMWSCS`, version 1 (`u16`),
the text (50 bytes), the source length (`u32` LE) and the source; it is
compiled again when read.

## Payloads

`docs/WORKMGMT.md`: `DeviceDescription` version 3 with the
`PrinterDevice` section, `OutputQueue` version 2 with the
`RemoteOutputQueue` section.

## Messages

| Case | Message |
|---|---|
| host name does not resolve | `TCP370A` |
| connection refused | `TCP3427` |
| no answer in time | `TCP3436` |
| other connect failure, TLS | `TCP3409` |
| connection broke while sending | `TCP3430` |
| protocol error or refusal | `TCP3701` |

The identifiers are `QTCPMSGF`'s; the texts are ours. Which ones IBM's
writers send is an open question.
