# TCP servers framework

Phase 10 section 5. `STRTCPSVR`, `ENDTCPSVR`, `STRTCP STRSVR(*YES)` and
`ENDTCP ENDSVR(*YES)` start and end every TCP server through one
framework: a registry of servers with IBM's job names, the hooks each
server implements, and the job that runs the command giving the hooks
its reach. The library is `net/servers`; the job side is
`work/qsjob/src/command/tcpsvr.rs` and the server roles in
`work/qsjob/src/work/`.

## Layout

| Path | What |
|---|---|
| `net/servers/src/registry/` | `ServerSpec`, `JobSpec`, `Readiness`; the table of every server (`table.rs`) |
| `net/servers/src/framework/` | `TcpServer` (the hooks), `Env` (the job's reach), `Framework`, `Starter` (the framework as `tcpcfg::server::ServerStarter`), the built-in hooks (`builtin.rs`) |
| `net/servers/src/attrs/` | typed attributes over the server table's keyword bag: `telnet.rs` (`CHGTELNA`), `ftp.rs` (`CHGFTPA`), `http.rs` (the administration ports), `ntp.rs` (`CHGNTPA`), `lpd.rs` (`CHGLPDA`), `smtp.rs` (`CHGSMTPA`) |
| `net/servers/src/listen.rs` | bind with retries, the accept loop with a stop flag, TLS handshake, `Service` |
| `net/servers/src/http/` | the minimal HTTP/1.1 administration listener: request head parsing, responses, the page, `Date` |
| `work/qsjob/src/command/tcpsvr.rs` | `JobEnv`, `STRTCPSVR`, `ENDTCPSVR`, `CHGTELNA`, `CHGFTPA`, `framework()` |
| `work/qsjob/src/command/tcp.rs` | `STRTCP`/`ENDTCP` start and end the autostart servers through `Starter` |
| `work/qsjob/src/work/telnet.rs` | role `TELNET`: `QTVTELNET` |
| `work/qsjob/src/work/httpadm.rs` | role `HTTPADM`: `ADMIN` |
| `net/ntp/` | SNTP: the packet codec, the client's exchange and poll, the adjustment rules, the server's answer and UDP loop |
| `net/lpd/` | the line printer daemon: RFC 1179 lines, the session against a `Spooler`, received jobs as spooled files, the queue state |
| `net/mail/` | mail out: addresses, MIME, the encodings, the SMTP client with `STARTTLS` and `AUTH` |
| `work/qsjob/src/work/ntp.rs`, `lpd.rs`, `netsvc.rs` | roles `NTP` (`QTOTNTP`) and `LPD` (`QTLPD00001`), and what they share |
| `work/qsjob/src/command/netcmd.rs` | `CHGNTPA`, `CHGLPDA`, `CHGSMTPA`, `SNDTCPSPLF`, `LPR`, `SNDSMTPEMM`, the job's `QtmmSendMail` |
| `work/qsjob/src/platform/clock.rs` | the system clock the SNTP client steps and slews |
| `work/qsjob/src/work/ftp.rs` | roles `FTP` (`QTFTP00001`) and `FTPSESS` (a session job); the server is `net/ftp` (docs/FTP.md) |

## The registry

Each server has one entry (`servers::registry::SPECS`). Job names,
users and subsystems are the oracle's (pub400, V7R5M0;
`docs/NET_OPEN_QUESTIONS.md` "Server jobs and subsystems"), except
`QTLPD00nnn`, which neither oracle runs (IBM's documentation names it;
it runs as `QTMPLPD`, the profile LPD spooled files belong to).
`nnn` in a name is a sequence number (`JobSpec::numbered`).

| Server | Jobs (user, subsystem) | Ports | Attributes | Application ID | Exit points | This release |
|---|---|---|---|---|---|---|
| `*TELNET` | `QTVTELNET` listener, `QTVDEVICE` workers (`QTCP`, `QSYSWRK`) | 23, 992 TLS | `CHGTELNA` | `QIBM_QTV_TELNET_SERVER` | `QIBM_QTG_DEVINIT`, `QIBM_QTG_DEVTERM` | built |
| `*HTTP` | `ADMIN` (`QTMHHTTP`, `QHTTPSVR`) | 2001, 2005 TLS | (bag `PORT`, `TLSPORT`) | `QIBM_HTTP_SERVER_ADMIN` | | built: `HTTPSVR(*ADMIN)` only |
| `*FTP` | `QTFTP00nnn` listener and session jobs (`QTCP`, `QSYSWRK`) | 21, 990 TLS | `CHGFTPA` | `QIBM_QTMF_FTP_SERVER` | `QIBM_QTMF_SERVER_REQ`, `QIBM_QTMF_SVR_LOGON` | built (docs/FTP.md) |
| `*SSHD` | `QP0ZSPWT` (`QUSER`, `QUSRWRK`) | 22 | `sshd_config` | | | slot |
| `*HOSTSVR` | `QZSOSMAPD`, `QZSOSGND`, `QZSCSRVSD`, `QZDASRVSD` (`QSERVER`), `QZHQSRVD`, `QPWFSERVSD` (`QSERVER`), `QNPSERVD`, `QZRCSRVSD`; prestart `QZSOSIGN`, `QZSCSRVS`, `QZDASOINIT`, `QZHQSSRV`, `QPWFSERVSO`, `QNPSERVS`, `QZRCSRVS` (`QUSER`) | 449, 8470-8476, 9470-9476 TLS | | (the `QIBM_OS400_QZBS_SVR_*` IDs) | | built (`servers::framework::HostSvr`) |
| `*NTP` | `QTOTNTP` (`QNTP`, `QSYSWRK`) | udp 123 | `CHGNTPA` | | | built |
| `*LPD` | `QTLPD00nnn` (`QTMPLPD`, `QSYSWRK`) | 515 | `CHGLPDA` | `QIBM_QSO_LPD_SERVER` | | built |
| `*DDM` | `QRWTLSTN` (`QUSER`, `QSYSWRK`), prestart `QRWTSRVR` (`QUSER`, `QUSRWRK`) | 446, 447, 448 TLS | `CHGDDMTCPA` | `QIBM_QRW_SERVER_DDM_DRDA` | | blocked |
| `*SMTP`, `*SNMP`, `*ROUTED`, `*BOOTP`, `*TFTP`, `*REXEC`, `*DHCP`, `*DNS`, `*POP` | | | | | | not planned (docs/PHASE10.md) |

`Readiness::Slot` means the server is being built elsewhere and starts
once its hooks are registered; `Blocked` names what it waits for;
`NotPlanned` servers answer `TCP1A04` to a start and are never active.

## Commands

- `STRTCPSVR SERVER(list) RESTART HTTPSVR INSTANCE`. `*ALL` is every
  server this release runs; `*AUTOSTART` those whose autostart is on.
  `HTTPSVR(*ADMIN)` (or `*ALL`, the default) is the administration
  instance; another name answers `TCP1A06`. `RESTART(*HTTP)` ends the
  HTTP instance first.
- `ENDTCPSVR SERVER(list) HTTPSVR INSTANCE`. `*ALL` is every running
  server.
- `CHGTCPSVR` stays what IBM's is: it changes a user-defined server of
  `ADDTCPSVR` (`tcpcfg`). A standard server's autostart is its `CHG*A`
  command's `AUTOSTART`.
- `STRTCP STRSVR(*YES)` asks the storage service for the autostart list
  (the server table's `QATOCSTART`) and starts each through
  `servers::framework::Starter`; `ENDTCP ENDSVR(*YES)` ends every
  running server first.

One line per server; messages are `QTCPMSG`'s: `TCP1A07` starting,
`TCP1A08` already started, `TCP1A0A` ending, `TCP1A09` not active,
`TCP1A04` not in this release (with what it waits for), `TCP1A05` its
job did not start, `TCP1A06` no such instance, `TCP1A10` attributes
changed. A command whose every server was refused ends in the first
refusal as its escape.

## Attributes

The storage service keeps a keyword bag per server in the TCP/IP server
table in `QUSRSYS` (`tcpcfg`'s `QATOCSVRA`; the autostart flag is
`QATOCSTART`'s), read and written over the link with `TcpOp::Servers`
and `TcpOp::SetServer`. `servers::attrs` types each server's bag.

`CHGTELNA` has the oracle's keywords, ranges and defaults (V7R5M0
`QCDRCMDD`, checked by `command/sys-cpps/tests/tcp_defs.rs`):
`AUTOSTART`, `NBRSVR`, `NBRTASK`, `TIMMRKTIMO`, `ENBWIDE`, `NBRFLDS`,
`DFTNVTTYPE`, `CCSID`, `TBLVTOUT`, `TBLVTIN`, `ALWSSL`. What the
server does with them:

| Keyword | Effect |
|---|---|
| `AUTOSTART` | whether `STRTCP` starts `QTVTELNET` |
| `ALWSSL` | `*NO` no port 992, `*ONLY` no port 23 |
| `TIMMRKTIMO` | a connection silent this long gets `IAC DO TIMING-MARK` (RFC 860); one that does not answer within as long again is ended (its job then follows `QDEVRCYACN`) |
| `NBRSVR`, `NBRTASK` | kept; each session is a `QTVDEVICE` thread of `QTVTELNET` |
| `ENBWIDE`, `NBRFLDS`, `DFTNVTTYPE`, `CCSID`, `TBLVTOUT`, `TBLVTIN` | kept; they govern 3270 and VT clients, which the server does not serve |

`PORT` in the same bag (no command sets it) moves the plain port; the
console's own TN5250 session (`lic/slic/src/ipl/link.rs`) reads it too.
The data area `QUSRSYS/QTVTELNA` of Phase 8 is no longer read.

## Server jobs

A server job is a job process started by the job that ran the command,
as a subsystem monitor is: `JobEnv::start_job` spawns `/qsjob` with the
registry's name, user, subsystem and role, kind `B`, `QSYS/QSYSJOBD`,
`QSYS/QSYSCLS20`, pool `*BASE`, and keeps it among the job's children.
`work/qsjob/src/start.rs` runs the role. A server job ends on an `End`
from its parent, `ENDJOB` or its subsystem ending (it polls
`sbs_state`); `ENDTCPSVR` ends a listener with `ENDJOB OPTION(*CNTRLD)`.
`*HTTP` starts `QHTTPSVR` (shipped, empty) when it is not active.

`servers::listen` is the accept loop a server job runs: `bind(port)`
retries for ten seconds while an ended server still holds the port;
`serve(listener, tls, service)` accepts until `Running::stop`, does the
TLS handshake for a secure port, and calls `Service::serve` on a thread
per connection.

## `*TELNET`

`QTVTELNET` reads `CHGTELNA`'s attributes at start. Each session runs
the exit points through the registration facility: before the device
is chosen, `QIBM_QTG_DEVINIT` `INIT0100` with the user, device,
connection and workstation type; a program that answers "do not allow"
ends the connection, and a device name it returns is the device asked
for. When the session ends, `QIBM_QTG_DEVTERM` `TERM0100`. The programs
run in a runtime job on `QTVTELNET`'s main thread (IBM runs them in the
`QTVDEVICE` job), built the first time a point has programs; a point
without programs costs one registry question. A session whose call
times out is refused.

## `*FTP`

`servers::framework::Ftp` starts the listener `QTFTP00001` with role
`FTP` in `CHGFTPA SBSD`'s subsystem (`QSYSWRK`), its ports (`PORT`,
`TLSPORT` of the server table's bag) in the request data; it is active
while `QTFTP00001` is, and `ENDTCPSVR` ends `QTFTP00001`, which ends its
session jobs. Each connection gets a `QTFTP00nnn` job (role `FTPSESS`)
that is handed the socket; docs/FTP.md has the server, its attributes
(`CHGFTPA`) and the `FTP` client.

## `*HTTP` administration

`ADMIN` serves `servers::http`'s page on 2001 and, with TLS, on 2005:
`GET` and `HEAD` of `/`, `/index.html` and `/HTTPAdmin` answer 200 with
a Quartz page saying web administration is not yet available, with the
system name and release; other paths 404, other methods 405, other
versions 505, a malformed head 400, a head over 16 KiB 431. Every
answer closes the connection. `servers::http::admin_tls` builds the TLS
configuration from the `*SYSTEM` store and the application ID
`QIBM_HTTP_SERVER_ADMIN` (`tls::config::for_app`); the job cannot open
the store yet (its stash key is the LIC partition's), so the job uses
the system's self-signed certificate of `work/qsjob/src/work/tls.rs`
until the storage service answers an application ID's identity.

## Adding a server

A server built later does three things:

1. Implements `servers::TcpServer`. Only `spec` is required; the
   defaults start the registry entry's listener job (`JobStart` with
   empty request data), end every listener with `ENDJOB`, and call the
   server active while its listener job is:

   ```rust
   pub trait TcpServer {
       fn spec(&self) -> &'static ServerSpec;
       fn jobs(&self, req: &Request, attrs: &ServerAttrs) -> Result<Vec<JobStart>, Refusal>;
       fn active(&self, env: &mut dyn Env, req: &Request) -> bool;
       fn start(&self, env: &mut dyn Env, req: &Request) -> Result<String, Refusal>;
       fn end(&self, env: &mut dyn Env, req: &Request, immediate: bool) -> Result<String, Refusal>;
   }
   ```

   `servers::framework::Standard::of("*NTP")` is a complete
   implementation for a server whose job reads its own attributes.
   Override `jobs` to pass request data (ports, instance) and `start`
   or `end` for anything more (`*HOSTSVR` starting one daemon per
   host server, `*SSHD` its PASE spawn job).
2. Sets its registry entry's listener `role` and `Readiness::Built` in
   `net/servers/src/registry/table.rs`, and registers its hooks in
   `work/qsjob/src/command/tcpsvr.rs::framework()`
   (`f.register(Box::new(...))`).
3. Adds its role to `work/qsjob/src/start.rs` with a
   `work/qsjob/src/work/<server>.rs` that runs the listener, usually
   `servers::listen::serve` with a `Service`, until its parent's `End`,
   `ENDJOB` or its subsystem ending.

The `Env` its hooks get:

```rust
pub trait Env {
    fn active(&mut self, job: &str) -> bool;
    fn start_job(&mut self, job: &JobStart) -> Result<(), String>;
    fn end_job(&mut self, job: &str, immediate: bool) -> Result<(), String>;
    fn attrs(&mut self, server: &str) -> ServerAttrs;
    fn start_subsystem(&mut self, name: &str) -> Result<(), String>;
}
```

## `*NTP`

`QTOTNTP` (`QNTP`, `QSYSWRK`; role `NTP`) is the SNTP client and server
(RFC 4330 over RFC 5905's header, `net/ntp`). `STRTCPSVR *NTP` runs
both; `STRTCP` runs the parts whose autostart is on (`AUTOSTART` for the
client, `SVRAUTOSTR` for the server; the server table's flag is on when
either is), told in the request data `CLIENT(*YES|*NO) SERVER(*YES|*NO)`.

`CHGNTPA` has the oracle's keywords, ranges and special values (V7R5M0
`QCDRCMDD`, `command/sys-cpps/tests/smallsvr.rs`); its defaults are
IBM's documented ones (the oracle's prompt could not show current
values).

| Keyword | Effect |
|---|---|
| `RMTSYS` | up to three time servers, tried in order each poll; the first usable answer is the sample (the shortest round trip of three exchanges) |
| `POLLITV` | minutes between polls; the attributes are read again at each poll |
| `MINADJ` | a smaller difference is left alone (`*NONE`: none is) |
| `ADJTHLD` | up to this the clock is slewed, beyond it set at once (`*MAXADJ`: never at once) |
| `MAXADJ` | a larger difference is not corrected; `QSYSOPR` gets `TCP1A41` |
| `ACTLOG` | `*CHANGE` logs each adjustment (`TCP1A40`), `*POLL` each poll too (`TCP1A45`) |
| `SVRACTLOG` | `*ALL` logs each request, `*ERROR` each unanswered one (`TCP1A46`) |
| `SYNCRQD` | `*YES`: the server answers only once the client has synchronized |
| `AUTOSTART`, `SVRAUTOSTR` | what `STRTCP` starts |

The clock is `CLOCK_REALTIME`, which the IPL set from the RTC and
`QDATE`/`QTIME` derive from (`lic/slic/src/ipl/clock.rs`). The job
(root, with `CAP_SYS_TIME`) adjusts it itself through
`work/qsjob/src/platform/clock.rs`: a step is `clock_settime`, a slew is
`adjtimex(ADJ_OFFSET_SINGLESHOT)` (the kernel moves the clock at most
500 ppm), and after a good sample the kernel is told the clock is
synchronized (`ADJ_STATUS`, `ADJ_MAXERROR`), which lets it keep the RTC
in step. `QTIMADJ` names the adjuster: the client claims it as
`QIBM_OS400_SNTP` when it is `*NONE`, gives it back when it ends, and
does not adjust (`TCP1A44` to `QSYSOPR`) while another value is there.
`QSYSOPR` also hears of a clock set at once (`TCP1A42`) and of the first
poll no server answered (`TCP1A43`).

The server answers mode-3 requests on UDP 123 from the system clock: at
the upstream's stratum plus one, with its address as the reference,
once the client has synchronized; else at stratum 10 as `LOCL`, or not
at all with `SYNCRQD(*YES)`.

## `*LPD`

`QTLPD00001` (`QTMPLPD`, `QSYSWRK`; role `LPD`) listens on 515 and
serves each connection on a thread with `net/lpd`'s RFC 1179 session.
What needs the store comes back to the job's thread, which holds the
link: whether a queue name is an output queue (`LIB/OUTQ`, or a name
found through the library list or in `QUSRSYS`; "receive a printer job"
is refused otherwise), spooling a job, and the queue state.

A job is spooled as soon as its control file and every data file it
prints have arrived (the acknowledgement of the file that completed it
says whether it was spooled); what is left when the connection ends is
spooled with the data that came. Each data file is one spooled file:

- from a QS/400 sender (`DESTTYPE(*OS400)`, `work/print`'s `YQS400=1`
  control file extension), the SCS and the sender's attributes as they
  were;
- text (`f`, `l`, `p`) through the line printer into SCS (form feeds as
  page breaks, tabs every eight, width 80, 132 or the longest line);
  FORTRAN carriage control (`r`) through its FCFC;
- anything else as received, device type `*USERASCII`.

The file is named from the control file's source name (`N`; `QPRTLPD`
without one), belongs to the LPD job (`QTMPLPD`) and has the sender's
user (`P`) as its `USRDTA`. The queue-state commands (03, 04) answer an
`lpq`-like list of the queue's files; "remove jobs" (05) answers that
files are removed with `DLTSPLF`; "print waiting jobs" (01) answers
nothing. `CHGLPDA` has the oracle's `AUTOSTART` and `NBRSVR` (kept: one
job serves every connection).

## `SNDTCPSPLF` and `LPR`

The same command under two names (the oracle's definitions match). The
job reads the spooled file through the storage service as `DSPSPLF`
reads it (request `splf_fetch`: the same authority check and `SF` audit
entry) and prints it with `work/print`'s LPR client, as a remote writer
would: `DESTTYP(*AS400)` sends the SCS and the attributes (`*OS400`),
`*OTHER` and `*PSF2` what `TRANSFORM` and `MFRTYPMDL` choose; `DESTOPT`
and `SEPPAGE` as for a remote output queue; `RMTSYS(*INTNETADR)` takes
`INTNETADR`; `DLTSPLF(*YES)` deletes the file once sent. `TCP1A49` says
it went, `TCP1A4A` why not.

## Mail out

`SNDSMTPEMM` builds a MIME message (`net/mail`): `From` the job's user at
the TCP/IP host and domain name, `To` and `Cc` from `RCP` (`*BCC`
recipients only in the envelope), `SUBJECT` (RFC 2047 when not ASCII),
the `NOTE` as text/plain or text/html (`CONTENT`) in UTF-8
quoted-printable, and each `ATTACH` file read from the IFS: `*TXT` ones
as UTF-8 text, the rest base64, with a media type from `*OCTET` (the
extension's), `*PLAIN`, `*HTML`, `*XML` or the one given. The
`QtmmSendMail` API sends a stream file that already holds a MIME
message to its `ADDT0100` recipients.

Both hand the message to the mail router `CHGSMTPA` names: `FWDHUBSVR`,
else `MAILROUTER`, either as `host[:port]` (25 by default). The SMTP
client (RFC 5321) says `EHLO` (`HELO` when refused), starts TLS with
`STARTTLS` when the relay offers it (implicit TLS on 465; the bag key
`SMTPTLS` makes it `*NONE`, `*STARTTLS` required or `*IMPLICIT`),
authenticates with `AUTH PLAIN` or `AUTH LOGIN` when the bag has
`AUTHUSER` and `AUTHPWD`, sends one `RCPT TO` per recipient and the
message dot-stuffed. Refused recipients are listed; the message goes
when one is accepted (`TCP1A4B`), else `TCP1A4C` says why. There is no
SMTP server; `CHGSMTPA` keeps its other attributes as given.

## `*DDM`

`net/ddm`'s application server (`server::Session` over `SqlBackend`,
`FileBackend` and `Authenticator`) is built; no implementation of those
three traits over the storage service exists yet, and a listener with
the null backends would refuse every request. `*DDM` stays
`Readiness::Blocked` until they exist: `QRWTLSTN` then listens on 446
(and 447, 448 with TLS) and hands each connection to a `QRWTSRVR` job
reading chains with `ddm::wire::read_chain` and answering with
`Session::handle`. `CHGDDMTCPA AUTOSTART` is kept in `QUSRSYS/QRWDDMTCPA`
by `net/ddm`, not in the server table; the two meet when `*DDM` is
built.

## Tests

- `cargo test -p servers`: the registry, the attribute codecs
  (`CHGTELNA` round trip, defaults equal to the oracle's prompt), the
  framework against a fake job (start, end, `*ALL`, `*AUTOSTART`,
  instances, refusals, a server registered later, `STRTCP`/`ENDTCP`
  through `tcpcfg`'s starter), HTTP parsing and routing, the `Date`
  header, and the administration listener over loopback sockets, plain
  and HTTPS with the application ID's certificate from a store in
  memory.
- `cargo test -p sys-cpps --test tcp_defs`: `CHGTELNA` against the
  oracle's definition.
- `cargo test -p sys-cpps --test smallsvr`: `CHGNTPA`, `CHGLPDA`,
  `CHGSMTPA`, `SNDTCPSPLF` and `LPR` against the oracle's definitions
  and prompts.
- `cargo test -p servers --test small`: `*NTP`'s parts and `*LPD`'s
  numbered listener.
- `cargo test -p ntp`: the packet codec and timestamps, offset and
  delay, unusable answers, the adjustment rules, the server's answer,
  and polls against servers on loopback (slew, step, refusal, silence).
- `cargo test -p lpd`: RFC 1179 lines, sessions in memory (either file
  order, abort, data to the end, refusals, queue state), received jobs
  as spooled files, and `work/print`'s LPR client into an output queue
  in memory over loopback.
- `cargo test -p mail`: addresses, encodings, MIME, and the SMTP client
  against a fake smart host (plain with `AUTH LOGIN`; `STARTTLS` with a
  certificate from a store in memory and `AUTH PLAIN`; refusals).
- QEMU step 69 (`tools/conformance/src/smallsvr.rs`): `STRTCPSVR *LPD`;
  a file sent from the host with the LPR client through a port forward
  shows on an output queue and in `DSPSPLF`; an unknown queue is
  refused; `CHGNTPA SYNCRQD(*NO)`, `STRTCPSVR *NTP` and an SNTP query
  from the host over a UDP forward, answered from the guest's clock;
  `ENDTCPSVR` of both.
- QEMU step 64 (`tools/conformance/src/servers.rs`): `STRTCPSVR *HTTP
  HTTPSVR(*ADMIN)` starts `ADMIN` in `QHTTPSVR`; the page arrives over
  2001 and 2005; a second start says already started, an unknown
  instance `TCP1A06`; `ENDTCPSVR` ends it and the port goes quiet;
  `ENDTCPSVR *TELNET` ends `QTVTELNET`, `CHGTELNA` takes new values and
  refuses an out-of-range one, `STRTCPSVR *TELNET` brings telnet back;
  `STRTCPSVR *LPD` answers `TCP1A04`. QEMU step 67 (`ftpsrv.rs`) runs
  the FTP server and client (docs/FTP.md).
