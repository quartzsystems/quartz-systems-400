# Network, server, IFS and QSH facts from the oracles

Captured read-only for Phase 10. Every file under `pub400/` was taken
from pub400.com (IBM i V7R5M0) on 2026-10-29 over JDBC (8471) and the
remote command server (8475) with JTOpen; the first line of each file
holds the query or command that produced it. The account name is
written `{user}`, this client's address `{client}`, and other users of
that shared system `{other}`. Nothing was changed on the system: the
work files (`{user}B/NETSPL`, `{user}B/NETFFD`, `/home/{user}/qsnet.out`)
were created and deleted in the same run.

The primary oracle (V7R1M0) answered no connection on 2026-10-24 (the
sign-on server dropped every connection) and had every host-server port
closed on 2026-10-29, so there is no `primary/` directory; the queries in the capture
script (`WRKACTJOB OUTPUT(*PRINT)`, `DSPSBSD`, `WRKREGINF OUTPUT(*PRINT)`,
`DSPFFD` of the `QUSRSYS` configuration files, QSH listings) wait for it.

| File | Source | What it holds |
|---|---|---|
| `jobs-by-subsystem.txt` | `QSYS2.ACTIVE_JOB_INFO` grouped | every active job of `QSYSWRK`, `QUSRWRK`, `QSERVER`, `QHTTPSVR`, the system jobs and the `QT*`/`QZ*`/`QRW*`/`QNP*`/`QPWF*` jobs: subsystem, name, user, type, function, count |
| `prestart-jobs.txt` | `QSYS2.PRESTART_JOB_INFO` | every prestart job entry of every subsystem description with its attributes |
| `dspsbsd-qusrwrk.txt`, `dspsbsd-qserver.txt`, `dspsbsd-qsyswrk.txt` | `DSPSBSD OUTPUT(*PRINT)` | the three server subsystems in full (pools, routing, prestart, autostart entries) |
| `subsystems.txt` | `QSYS2.SUBSYSTEM_INFO` | the subsystems' attributes |
| `server-routing.txt` | `QSYS2.SERVER_SBS_ROUTING` | the columns of the server routing view (no rows on pub400) |
| `netstat-listen.txt`, `netstat-listen-jobs.txt` | `QSYS2.NETSTAT_INFO`, `QSYS2.NETSTAT_JOB_INFO` | every listening port with its service name, binding user and owning job |
| `netstat-interfaces.txt`, `netstat-routes.txt`, `tcpip-info.txt` | `QSYS2.NETSTAT_INTERFACE_INFO`, `QSYS2.NETSTAT_ROUTE_INFO`, `QSYS2.TCPIP_INFO` | the interfaces, routes and the host identity with every column |
| `config-objects.txt`, `config-objects-qsys.txt` | `QSYS2.OBJECT_STATISTICS` | the TCP/IP, server and DCM configuration objects in `QUSRSYS` and `QSYS` with type, attribute and text |
| `config-file-formats.txt`, `config-file-fields.txt` | `DSPFFD OUTPUT(*OUTFILE)` | the record formats (name, length, field count) and every field (offset, length, digits, type, text) of the `QATOC*`, `QATM*`, `QATO*`, `QAZSPP*`, `QAOKP*`, `QAZD*` physical files |
| `exit-points.txt`, `exit-programs.txt` | `QSYS2.EXIT_POINT_INFO`, `QSYS2.EXIT_PROGRAM_INFO` | every registered exit point and format, and the registered exit programs |
| `commands.txt`, `command-definitions.txt` | `QSYS2.COMMAND_INFO`, `QCDRCMDD` (`CMDD0100`) | the Phase 10 command inventory in `QSYS`, and 155 definitions summarized from the API's XML: command attributes, every PARM/ELEM/QUAL with type, length, defaults, restrictions, choices and special/single values; 42 definitions the profile may not retrieve (`CPF9802`) or that V7R5M0 lacks (`CPF9801`: `ADDTCPRSI`, `RMVTCPRSI`, `CVTTCPCL`) are listed as not retrieved |
| `messages-tcp.txt` | `QSYS2.MESSAGE_FILE_DATA` | `QTCP/QTCPMSGF`: every `TCP*` message id with severity, substitution data count and layout, first-level text |
| `messages-cpf.txt` | `QSYS2.MESSAGE_FILE_DATA` | `QCPFMSG` ids of the IFS, communications, TCP/IP, DDM, printer and server areas (`CPFA0*`, `CPFA1*`, `CPFA9*`, `CPDA0*`, `CPIA*`, `CPCA*`, `CPF26*`, `CPI26*`, `CPF3C*`, `CPF57*`, `CPF58*`, `CPF59*`, `CPI8*`, `CPF9*`, `CPFB*`, `CPE*`, ...) with severity and substitution data layout |
| `messages-qsh.txt`, `message-files-qsys.txt`, `message-files-other.txt`, `message-files2.txt`, `msgf-columns.txt` | `QSYS2.MESSAGE_FILE_DATA`, `OBJECT_STATISTICS` | the `QSHELL` message file, the message files of `QSYS`, `QUSRSYS`, `QSHELL`, `QHTTPSVR`, `QSSL`, `QICSS`, `QOPENSYS`, `QIWS` and `QTCP`, the view's columns |
| `qsh-*.txt` | `QSH CMD(...)` with the output copied back | `uname`, `id`, `env`, `ls -la` of `/usr/bin` (163 entries), `/QOpenSys/usr/bin`, the PASE directory, the OpenSSH product and configuration directories, the non-comment lines of each `sshd_config`, the DCM store directories, `/`, `/QOpenSys`, `/etc`, the shells; `qsh-netstat.txt` records that PASE has no `netstat` |
| `envvars.txt`, `sysvals-net.txt` | `QSYS2.ENVIRONMENT_VARIABLE_INFO`, `QSYS2.SYSTEM_VALUE_INFO` | the system-level environment variables, the network and security system values |
| `views-columns.txt` | `QSYS2.SYSCOLUMNS` | the columns of the QSYS2 views used here, for QS/400's own catalog |
| `cl-log.txt` | the remote command server | every command run with its messages: the `PING` messages (`TCP3203`, `TCP3204`, `TCP3215`, `TCP3211`, `TCP3210`, and `TCP3202` for `::1`) and the work-file commands |

`WRKSRVTBLE`, `WRKPCLTBLE` and `WRKTCPHTE OUTPUT(*PRINT)` are refused
from the remote command server (`CPD0031`); the service, protocol and
network tables are captured as screens under `golden/phase10/tcpcfg/`.
