# QAP: Quartz Access Protocol, version 1 (draft)

QAP is how QS/400 Access talks to QS/400 for everything except 5250 and printer sessions, which stay on TN5250E (RFC 4777). It replaces the plan to clone the host-server protocol. It is an original Quartz design: nothing in it is taken from IBM host-server protocols, JTOpen or any IBM-authored source.

This draft settles the decisions below. Anything it leaves open is listed under Open questions; the first agent to reach one stops and asks Cody.

## Decisions

| Topic | Decision |
| --- | --- |
| Transport | TCP, one port, default **2320**, configurable on both ends |
| Security | TLS 1.3 only, no plaintext mode. Server certificate from the QS/400 certificate manager, application ID `QS400_ACCESS_SERVER`. The client validates it with its existing untrusted-certificate trust flow |
| Framing and encoding | Wire, from the QS/400 repository's `ipc` crate, the same framing and serialization QSI uses. Types below are Wire types; enums use `wire_enum!`. If Wire cannot express something below, stop and ask; do not invent a second encoding |
| Where the code lives | `qap-proto` (envelope and message types) lives in the QS/400 repository beside `ipc`. QS/400 Access pins both by commit |
| Multiplexing | One connection carries many concurrent requests. Every request has a service ID and a request ID; replies carry the request ID. No port mapper and no per-service ports |
| Versioning | Client hello sends the range of protocol versions it supports; server picks one or refuses. Version 1 is this document |
| Server jobs | Listener job `QAPD` in subsystem `QSYSWRK`; one job per connection in `QUSRWRK`, running as `QUSER` until sign-on and as the signed-on user profile after it |
| Server control | `STRTCPSVR SERVER(*QAP)`, `ENDTCPSVR SERVER(*QAP)`, autostart attribute; shown on Navigator's Servers page |

## Envelope (all versions)

Every frame is one Wire-encoded `Envelope`. The envelope layout never changes between protocol versions; versions change message bodies and add operations.

| Field | Type | Meaning |
| --- | --- | --- |
| `kind` | enum `Kind` | `Request` = 0, `Reply` = 1, `Event` = 2, `Cancel` = 3 |
| `request_id` | u32 | Chosen by the client, non-zero, unique among its outstanding requests on the connection; reusable after the final reply. Events use 0. `Cancel` names the request to cancel |
| `service` | enum `Service` | `Connection` = 0, `SignOn` = 1, `System` = 2. Later phases append values; never renumber |
| `op` | u16 | Operation within the service (tables below) |
| `flags` | u16 bit set | bit 0 `More`: further reply frames follow for this request. All other bits zero in version 1 |
| `status` | enum `Status` | Replies and events only; `Ok` on requests |
| `message` | Option of `ServerMessage` | Present when the reply or event carries a QS/400 message |
| `body` | bytes | The Wire-encoded body for this service and operation; empty when the operation has none |

`ServerMessage` = { `id`: String (seven characters, for example `CPF2204`), `severity`: u8, `data`: bytes (substitution data), `text`: String (first level), `help`: String (second level: cause and recovery) }, in the user's language once signed on, the system's before.

**Status values (u16).** `Ok` 0 · `Error` 1 (the message says what) · `VersionUnsupported` 2 · `NotSignedOn` 3 · `SignOnFailed` 4 · `PasswordExpired` 5 · `NotAuthorized` 6 · `NotFound` 7 · `InvalidRequest` 8 · `Busy` 9 · `Cancelled` 10 · `SessionExpired` 11 · `ServerEnding` 12. Sign-on failures carry the same message a 5250 sign-on would show for the same case, and nothing more.

**Limits.** A frame is at most 1 MiB; larger results are sent as several reply frames with `More` set on all but the last. At most 64 outstanding requests per connection; beyond that the server replies `Busy`. The client sends `Ping` after 60 s without traffic; the server closes a connection after 300 s without frames. Closing a connection does not end the session: it can be resumed until its idle timeout.

**Events (server to client, `request_id` 0).** `Connection` op 100 `ServerEnding` (status `ServerEnding`, message says why; the server closes after sending it) · `SignOn` op 100 `SessionEnded` (status `SessionExpired`, or `Ok` when ended by an operator, with a message).

## Phase 0 (this round): connection, sign-on, server information

**Operations.** Strings are UTF-8. Times are i64 milliseconds since 1970-01-01 UTC. Bodies not listed as a reply are empty.

| Service | Op | Request body | Reply body | Allowed before sign-on |
| --- | --- | --- | --- | --- |
| Connection | 1 `Hello` | `min_version` u16, `max_version` u16, `client_name` String, `client_version` String, `capabilities` list of String | `version` u16, `system_name` String, `release` String, `services` list of `Service`, `capabilities` list of String, `signon_methods` list of enum (`Password` = 0), `server_time` i64, `time_zone` String, `system_ccsid` u16 | yes; must be the first request, `request_id` 1 |
| Connection | 2 `Ping` | — | `server_time` i64 | yes |
| SignOn | 1 `SignOn` | `user` String, `password` String | `SignOnReply` | yes |
| SignOn | 2 `ChangePassword` | `user` String, `old_password` String, `new_password` String | `SignOnReply` | yes |
| SignOn | 3 `Resume` | `token` 32 bytes | `SignOnReply` (same token, timeout restarted) | yes |
| SignOn | 4 `SignOff` | — | — | no |
| System | 1 `SystemInfo` | — | `system_name` String, `release` String, `serial` String, `model` String, `ipl_time` i64, `products` list of { `id` String, `option` String, `release` String } | no |

`SignOnReply` = { `token` 32 bytes, `idle_timeout_secs` u32, `profile` String, `text` String, `ccsid` u16, `date_format` String, `date_separator` String, `time_separator` String, `home_directory` String, `special_authorities` list of String }.

**Rules.**

- A `Hello` that is not the first request, or a second `Hello`, gets `InvalidRequest` and the connection closes. No version in the client's range gets `VersionUnsupported`, then the connection closes.
- `SignOn` applies the same checks as a 5250 sign-on: `QMAXSIGN` and its action, disabled profiles, password expiry, `*NONE` passwords refused, and the same audit journal entries. An expired password gets `PasswordExpired` and no token; the client then sends `ChangePassword`, which applies the system password rules and on success returns a `SignOnReply`.
- `ChangePassword` is also allowed in a signed-on session for that session's own profile.
- The token is 256 random bits, stored server-side only. `Resume` re-attaches a new connection to the session until the idle timeout; after it, `SessionExpired`. `SignOff` ends the session and invalidates the token.
- Anything other than the operations marked "yes" before sign-on gets `NotSignedOn`. Before sign-on the connection job runs as `QUSER`; after it, as the signed-on profile.
- The client never logs or stores passwords or tokens, and clears them from memory after use.

## Later phases (spec now, build later)

The client agent inventories which host-server functions QS/400 Access was going to use, and adds each as a QAP service with its requests and replies to this document before building it. Expected, in this order: remote command (run CL, return messages and the job), spooled files (list, read as SCS, text or PDF, hold, release, delete, move), SQL, IFS files, message queues. Optional: a one-time `TelnetTicket` from a signed-on session that the Telnet server accepts for 5250 auto sign-on, which would remove the need for password substitution on Telnet.

## Open questions

1. The `QAPD` job and `*QAP` server names, port 2320 and the limits above. Confirm or change before the server ships.
2. Whether QS/400 Access must still reach real IBM i systems for anything other than 5250 and printing. This spec assumes no.
