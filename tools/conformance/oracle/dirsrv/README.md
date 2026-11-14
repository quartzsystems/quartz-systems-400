# Directory server facts from the oracles

Captured read-only on 2026-11-13 for the directory server handoff
(`docs/DIRSRV.md`). Nothing was changed on either system: the probes
wrote only to `QTEMP` and, for the QSH tools' usage, to files in
`/home/QSTEST` that were read back and deleted. LDAP was read
anonymously (no bind with a password, so no failed sign-on can count
against `QMAXSIGN`). The system's name is masked as `{system}` (its DNS
name), `{host}` and `{domain}`, the server ID as `{server-id}`.

## What each oracle can show

| Oracle | Directory server | What it settles |
|---|---|---|
| primary (V7R1M0) | **running**: instance `QUSRDIR`, IBM Directory Server 6.1, RDBM backend in `QUSRDIRDB`; 389 open, 636 refused (no certificate assigned) | the wire: the root DSE, `cn=schema`, result codes, what anonymous users see; the commands and prompts; the configuration layout; the QSH tools; the message IDs and formats; the jobs |
| pub400 (V7R5M0) | not running (389, 636 refused); the account is not authorized to `STRTCPSVR`, `ENDTCPSVR`, `CHGDIRSVRA` | the 7.5 objects, `QGLDCLNT`'s exports, the 7.5 message IDs, that no SQL service covers LDAP, the function usage ID |

So unlike NFS, wire behaviour can be diffed against a live IBM server,
anonymously. Anything that needs a bind (writes, ACL evaluation, the
projected backend's entries, password policy) is `NEEDS-CODY` in
`docs/DIRSRV.md` with the command sequence: it needs the administrator
password or a profile bind on the primary, which this project does not
try (see below).

## Files

| File | Source | What it holds |
|---|---|---|
| `probes/LdapProbe.java` | | ports, root DSE, `cn=schema`, objects, commands, message files, QSYSINC members, SQL services, files, jobs (both oracles) |
| `probes/LdapShape.java` | | anonymous base and one-level searches of every naming context and of `cn=changelog`, `cn=configuration`, `cn=monitor`; StartTLS; whoami; the instance directory; the server's jobs; `QDIRSRV` |
| `probes/LdapFiles.java` | | the configuration files and the QSYSINC/H members, copied out of the tree |
| `probes/LdapTools.java` | | `TOOL -?` for each QSH tool; `QMHRTVM RTVM0400` for each `QGLDMSG` ID |
| `rootdse.ldif` | anonymous base search of `""`, `*` and `+` | the primary's root DSE |
| `ldaptest-primary.txt` | `ldaptest oracle` (`tools/ldaptest`) | the anonymous read-only scenarios' answers: scopes, filters, limits, attribute selection, compare, the controls and extended operations, result codes; what QS/400's answers are diffed against |
| `schema-primary.txt` | anonymous base search of `cn=schema` | 1627 attribute types, 428 object classes, 43 syntaxes, 34 matching rules; `DESC` removed |
| `ibmslapd-shape.ldif` | `/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/ibmslapd.conf` | the instance configuration's entries and attribute names with non-secret values; comments dropped, every password and the password policy values masked |
| `idsinstances.ldif` | `/QIBM/UserData/OS400/DirSrv/idsinstinfo/idsinstances.ldif` | the instance list (`QUSRDIR`, autostart) |
| `cmdd71/*.xml` | `QCDRCMDD` through JTOpen's `Command.getXML` on the primary | `CHGDIRSVRA`, `STRTCPSVR`, `ENDTCPSVR` |
| `messages.txt` | `QMHRTVM RTVM0400` over `QSYS/QGLDMSG` on the primary | 481 IDs (`GLD0101`-`GLD0956`), severity, substitution formats |
| `messages-pub400.txt` | JTOpen `getMessage` on pub400 | the 7.5 IDs and severities (587 to `GLD0960`) |
| `qsh-tools.txt` | `TOOL -?` in QSH on the primary | each tool's synopsis and option letters |
| `api-names.txt` | `QSYSINC/H(LDAP, LBER, LDAPSSL)` on the primary; `QSYS2.PROGRAM_EXPORT_IMPORT_INFO` on pub400 | function names only (no header content): 185 in `LDAP`, 16 in `LBER`, 2 in `LDAPSSL`; the 205 exports of `QSYS/QGLDCLNT` at 7.5 |
| `objects-primary.txt`, `objects-pub400.txt` | `DSPOBJD` to `QTEMP`; `QSYS2.OBJECT_STATISTICS` | the directory's objects, the commands, the libraries, `QUSRDIRDB`'s tables |
| `../../golden/dirsrv/` | `tn5250-driver run --target oracle` with `../../probes/dirsrv/prompts-dirsrv.tn` | `CHGDIRSVRA` (prompt, F10, Page Down, help), `STRTCPSVR SERVER(*DIRSRV)` and `ENDTCPSVR SERVER(*DIRSRV)` (prompt, F10, Page Down), the `CMDTCP` menu |

## Findings

### The server, its job and its configuration

- One server instance per name; the shipped one is `QUSRDIR`
  (`idsinstances.ldif`: `ids-instanceAutostart: 1`, location
  `/QIBM/UserData/OS400/DirSrv`, version 6.1). Its job is named after
  the instance: `QUSRDIR`, user `QDIRSRV`, type batch, in `QSYSWRK`,
  job description `QSYS/QDIRSRV`. The handoff's "job `QDIRSRV`" is the
  user and the job description.
- Two more jobs run beside it in `QSYSWRK` as `QDIRSRV`: `QGLDPUBE`
  and `QGLDPUBA` (job descriptions of the same names), the publishing
  agents. `QGLDPUBE` is a program and a job, not a command.
- `QDIRSRV` is a `*USER` profile, enabled, no special authorities, UID
  4294770712 (in the shipped range `docs/NFS.md` recorded).
- The configuration is LDIF: `/QIBM/UserData/OS400/DirSrv/idsslapd-
  QUSRDIR/etc/ibmslapd.conf` (CCSID 1208) with `cn=Configuration` and
  its children (`cn=Front End`, `cn=Connection Management`, `cn=Digest`,
  `cn=Event Notification`, `cn=Log Management` with `cn=Audit`,
  `cn=Default`, `cn=ibmslapd`, `cn=Replication`, `cn=pwdPolicy Admin`,
  `cn=Replication`, `cn=Schemas` with the config, LDCF, RDBM and System
  backends, `cn=SSL`, `cn=Transaction`, `cn=Persistent Search`,
  `cn=Passthrough Authentication`). The schema files `V3.*.at`, `V3.*.oc`,
  `V3.ldapsyntaxes`, `V3.matchingrules` and `V3.modifiedschema` sit in
  the same `etc/`; the shipped copies are in
  `/QIBM/ProdData/OS400/DirSrv/etc/SchemaV6.2/` (CCSID 437).
- **The administrator DN is `cn=Administrator`** (`ibm-slapdAdminDN`), not
  `cn=root`; `ibm-slapdAdminGroupEnabled: false`.
- Defaults recorded there: size limit 500, time limit 900, aliases
  dereferenced `always`, anonymous binds allowed, idle timeout 300 s,
  write timeout 120 s, schema check `V3_lenient`, persistent search on
  (at most 100), transactions on (20, 5 operations each, 300 s), paged
  results allowed to non-administrators (limit 2 at once), sort allowed
  to non-administrators (3 keys), audit off with bind and unbind logged
  when on, password encryption one of
  `none/aes128/aes192/aes256/crypt/sha/ssha/md5`, secure port 636 with
  `ibm-slapdSecurity: none` and no certificate. `ibm-slapdPwEncryption`'s
  value is masked.
- The backends are named by service program and entry point
  (`ibm-slapdPlugin: database /QSYS.LIB/QGLDRDBM.SRVPGM
  rdbm_backend_init`). Suffixes: `cn=configuration` (config backend),
  `cn=schema` (LDCF), `cn=localhost`, `cn=ibmpolicies` and the
  `dc={host},dc={domain}` suffix (RDBM), `os400-sys={system}` (the
  system projected backend, `QGLDPSYS`, `ibm-slapdOs400UsrprjRead: TRUE`).
  The SASL plug-ins listed: `CRAM-MD5`, `GSSAPI`, `OS400-PRFTKN`,
  `DIGEST-MD5`; the root DSE advertises only `DIGEST-MD5` and
  `OS400-PRFTKN`.
- `/QIBM/UserData/OS400/DirSrv/` also holds `ldap_server_info.conf`
  (one byte), `IDSWebApp/` and its log (the web administration tool).
  There is no `ldap.conf` anywhere under `DirSrv`, `/etc` or
  `/QOpenSys/etc`; `/QIBM/ProdData/OS400/DirSrv/ibmldap.conf` holds the
  client's SASL plug-in lines.
- The RDBM backend's database is the library `QUSRDIRDB`: `LDAP_ENTRY`,
  `LDAP_DESC`, one table per indexed attribute (`CN`, `SN`, `UID`, `MAIL`,
  `OBJECTCLASS`, `IBMENTRYUUID`, the `PWD*` set, ...) and the ACL and
  owner tables `QAGLDACL*`, `QAGLDOWN*`. `QUSRDIRCF` is the configuration
  library. QS/400 keeps entries in the object store instead (a documented
  difference: nobody reads these tables directly).

### Commands

- There is exactly one directory server command at V7R1M0:
  **`CHGDIRSVRA INSTANCE(name 8, QUSRDIR) DN(*SAME | (dn 50 *ADMINDN
  *UPDATEDN, password 50))`**, processing program `QGLDCHGATR`, help
  panel group `QHGLDCMD` (whose help reads "not available" on the
  primary). No `WRKDIRSVRA`, `DSPDIRSVRA`, `CFGDIRSVR`, `CHGLDAPA`,
  `STRDIRSVR`, `ENDDIRSVR` on either release. Everything else is
  Navigator, the web administration tool and the QSH tools.
- `STRTCPSVR SERVER(*DIRSRV)` and `ENDTCPSVR SERVER(*DIRSRV)` take
  `INSTANCE` (prompted for `*DIRSRV`): server instance `*DFT`, `*ALL`,
  `*AUTOSTART` or a name, and instance startup values; `TCP1617` is the
  dependency message. The captures show the prompt as IBM draws it.
- `CMDTCP` is unchanged by the directory (no directory entry on its
  first page).
- The `*DIRE`/`CHGSYSDIRA`/`WRKDIRE` family is the SNADS system
  distribution directory, not LDAP: out of scope.

### The wire (root DSE and anonymous answers)

- `supportedLDAPVersion` 2 and 3; `vendorName` "International Business
  Machines (IBM)", `vendorVersion` and `ibmdirectoryversion` 6.1;
  `ibm-slapdServerBackend: RDBM`; `security: none`; `port: 389`;
  `subschemaSubentry: cn=schema`; `ibm-slapdisconfigurationmode: FALSE`;
  `ibm-osregistrycontext: OS400-SYS={system}`.
- `namingContexts`: `CN=SCHEMA`, `CN=LOCALHOST`, `CN=IBMPOLICIES`,
  `DC={host},DC={domain}`, `OS400-SYS={system}`, upper-cased as IBM
  writes them. `cn=changelog` is not configured (32 noSuchObject).
- 30 `supportedExtension` values (StartTLS `1.3.6.1.4.1.1466.20037`,
  the IBM `1.3.18.0.2.12.*` set, the Netscape
  `2.16.840.1.113730.3.4.3` repeated) and 21 `supportedControl` values
  (paged results `1.2.840.113556.1.4.319`, sort `.473`, tree delete
  `.805`, manageDsaIT `2.16.840.1.113730.3.4.2`, persistent search
  `.3`, proxied authorization v2 `.18`, the IBM `1.3.18.0.2.10.*` set).
  **Not supported: whoami (`1.3.6.1.4.1.4203.1.11.3` answers 2 "unsupported
  extended operation"), password modify (RFC 3062 is absent; IBM's
  `ldapchangepwd` modifies `userPassword`), pre/post read, assertion.**
- StartTLS with no certificate assigned answers protocolError (2).
- Anonymous reads: `cn=localhost` and `cn=ibmpolicies` with their ACL
  operational attributes (`aclEntry`, `aclPropagate`, `aclSource`,
  `entryOwner`, `ownerPropagate`, `ownerSource`, `ibm-effectiveAcl`,
  `ibm-entryuuid`, the replication flags, `ibm-capabilitiessubentry`);
  `cn=pwdpolicy,cn=ibmpolicies` (objectclasses `pwdPolicy`,
  `ibm-pwdPolicyExt`, `ibm-pwdGroupAndIndividualPolicies`) with its
  attributes; `cn=monitor` with 90-odd counters; `cn=configuration` is
  50 insufficientAccessRights; the empty RDBM suffix is 32.
- **The projected backend shows anonymous users only its root**:
  `os400-sys={system}` with objectclass `os400-root`; one level below and
  `cn=accounts,os400-sys=...` return nothing. The schema names the
  projected classes: `os400-usrprf` (`MUST os400-profile`, the profile's
  attributes as `os400-*`), and the others under `1.3.18.0.2.6.39x`
  (`schema-primary.txt`). The bind DN form
  `os400-profile=NAME,cn=accounts,os400-sys={system}` is IBM's documented
  one and is not proven here: `NEEDS-CODY`.

### Client, tools, messages, SQL

- `QSYS/QGLDCLNT` (and `QGLDCLNT64` at 7.5) exports the client API;
  `api-names.txt` lists the names. `QSYSINC/H` members: `LDAP`, `LBER`,
  `LDAPSSL`, `QGLDPAPI` (the publishing API), `QGLDUAPI`, `QGLDCPYVL`,
  `QGLDSSDD`; `QRPGLESRC` and `QCBLLESRC` carry only `QGLDPAPI` and
  `QGLDUAPI`. There is no RPG copy member for the LDAP client.
- QSH ships `ldapadd`, `ldapmodify`, `ldapsearch`, `ldapdelete`,
  `ldapmodrdn`, `ldapchangepwd`, `ldapexop`, `ldapdiff`, `ldapcompare` in
  `/usr/bin`. `ldapexop -op` takes `acctstatus`, `cascrepl`, `clearlog`,
  `controlqueue`, `controlrepl`, `controlreplerr`, `effectpwdpolicy`,
  `evaluategroups`, `getattributes`, `getlogsize`, `getusertype`,
  `locateEntry`, `onlineBackup`, `quiesce`, `readconfig`, `readlog`,
  `repltopology`, `resumerole`, `stopserver`, `unbind`, `uniqueattr`.
- Messages are `GLDxxxx` in `QSYS/QGLDMSG` (481 at 7.1, more at 7.5); no
  `QGLDCMSG` or `QGLDSMSG`.
- SQL: no `QSYS2` routine or view names LDAP or the directory server at
  7.5 (`SQLTOOLS.LDAP_*` on pub400 is a user schema). Function usage:
  `QIBM_DIRSRV_ADMIN`. No exit point names `GLD`, `LDAP` or `DIRSRV`.
- The product: 5770SS1 option 3, "Extended Base Directory Support"
  (pub400's `SOFTWARE_PRODUCT_INFO`). QS/400 ships it in `*BASE`.

## What was not tried, and why

A bind as `cn=Administrator` needs its password, which this project
does not hold; a bind as an OS profile would be QSECOFR's, and a wrong
DN form could be a failed sign-on. Both wait for Cody: the sequences
are in `docs/DIRSRV.md`'s `NEEDS-CODY` list.
