# Directory Server: using it

QS/400's directory server is an LDAP version 3 server in the base
operating system. This guide is for the people who run it; the design
is docs/DIRSRV.md, whose sections it cites.

## Overview

A system has one or more directory server **instances**, listed in
`/QIBM/UserData/OS400/DirSrv/idsinstinfo/idsinstances.ldif`. Every
system has `QUSRDIR`. Each instance:

- runs as a batch job named after it (`QUSRDIR`), user `QDIRSRV`, in
  subsystem `QSYSWRK`;
- is configured by its own `ibmslapd.conf`
  (`/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/ibmslapd.conf`);
- keeps its entries in an object of its own in library `QUSRDIRDB`,
  saved with `SAVLIB LIB(QUSRDIRDB)`.

An instance serves these **suffixes** (top names): `cn=localhost` and
`cn=ibmpolicies` (the server's own), `os400-sys=` followed by the
system's DNS name (the system's user profiles, read only), `cn=schema`,
`cn=configuration`, `cn=monitor`, and the suffixes you add for your own
entries, such as `dc=example,dc=com`.

Start and end an instance with

```
STRTCPSVR SERVER(*DIRSRV) INSTANCE(QUSRDIR)
ENDTCPSVR SERVER(*DIRSRV) INSTANCE(QUSRDIR)
```

`INSTANCE(*DFT)` is `QUSRDIR`, `*ALL` every instance, `*AUTOSTART`
those set to start automatically. With `*DIRSRV` an autostart server
(`CHGTCPSVR`), `STRTCP` starts the autostart instances. Navigator's
Network > Directory server page lists the instances with Start, End and
Restart.

## Configuring

Everything an instance does is in its `ibmslapd.conf`, an LDIF file.
Change it in one of three ways:

- Navigator, Network > Directory server, an instance's **Properties**,
  **Suffixes**, **Indexes**, **Schema** and **Access** tabs;
- QAP's `Dirsrv` service (docs/QAP.md, service 14), for programs;
- LDAP itself: the administrator may modify the entries under
  `cn=configuration`.

The file keeps its order and comments through every change. A change is
used when the instance next starts, except the administrator's password,
the limits and logging, which apply at once; Navigator's **Restart**
ends and starts the instance.

What QS/400 honours is docs/DIRSRV.md §10: ports and security, the size
and time limits, idle and write time-outs, anonymous binds, alias
dereferencing, schema checking (`V3_lenient` or `V3`), the password
storage scheme (`ssha512` by default), read only, paging and sorting
limits, persistent search, logging, auditing, the change log, the
password policy, the projected backend and default referrals, and the
indexes (`ibm-slapdIndex: attribute:pes`, presence, equality and
substring keys). An attribute QS/400 does not honour may stay in the
file only at the value it was shipped with; another value stops the
start with message `GLD0126` naming it, and a value of an honoured
attribute that QS/400 does not take with `GLD0232`.

### Suffixes and entries

Add a suffix of your own on the Suffixes tab (or with another
`ibm-slapdSuffix` line on the `cn=Directory` entry of the file), start
the instance again, then add its top entry and the rest with `ldapadd`
or any LDAP client. A suffix may not lie inside or above another.
Removing a suffix takes it out of the configuration only; its entries
stay in the store and come back if it is added again.

### Schema

The schema is QS/400's own files plus the changes made through
`cn=schema`, kept in the instance's `V3.modifiedschema`. Add an
attribute type or object class on the Schema tab, typed as its
description, with an OID of your organization's own arc. Definitions
the server needs cannot be removed.

## CHGDIRSVRA

`CHGDIRSVRA` changes the administrator of an instance or a password:

```
CHGDIRSVRA INSTANCE(QUSRDIR) DN(*ADMINDN 'new password')
CHGDIRSVRA INSTANCE(QUSRDIR) DN('cn=Directory Admin' 'new password')
CHGDIRSVRA INSTANCE(QUSRDIR) DN(*UPDATEDN 'new password')
```

The first sets the administrator's password; the second makes another
DN the administrator, with its password; the third sets the password of
the replication update DN, which the shipped configuration does not
name (`GLD0215`). Passwords are kept as salted SHA-512 hashes. The
command needs special authorities `*ALLOBJ` and `*IOSYSCFG` and ends
with `GLD0123`. Navigator's Properties tab does the same under
**Administrator**.

Until the administrator has a password nobody can bind as it: set one
before you use the directory.

## Security

- **Who may administer.** The administrator DN, and any user profile
  with function usage `QIBM_DIRSRV_ADMIN` allowed, bind with every
  right. Reading the configuration through Navigator or QAP needs
  `*IOSYSCFG`; changing it `*ALLOBJ` and `*IOSYSCFG`.
- **Binds.** A DN under one of your suffixes binds with its
  `userPassword`. A user profile binds as
  `os400-profile=NAME,cn=accounts,os400-sys=SYSTEM` with its own
  password, under the same rules as a sign-on (`QMAXSIGN`, disabled
  profiles, expired passwords, `PW` audit entries). With TLS, a client
  certificate can bind with SASL `EXTERNAL`. Anonymous binds are allowed
  unless **Allow anonymous binds** is off.
- **TLS.** Assign a certificate to application ID
  `QS400_DIRECTORY_SERVER` in the certificate store, then set
  **Security** to `SSL` (LDAPS on 636), `TLS` (StartTLS on 389) or
  `SSLTLS` (both). Without a certificate StartTLS is refused.
- **Access control.** Each entry has an ACL of its own or inherits one
  (`aclEntry`, `aclPropagate`), and owners (`entryOwner`). With none,
  everyone may read, search and compare normal attributes. The Access
  tab shows where an entry's ACL comes from and changes it.
- **Auditing.** With `ibm-audit` on (Properties, **Audit**), the
  operations switched on under `cn=Audit` are written to `QAUDJRN` as
  `DI` entries.
- **Checks.** `QSSECCHK` reports anonymous binds allowed, simple binds
  over plain LDAP, and an administrator password unset or well known.

## Publishing

The publishing agents copy the system's information into a directory:
the users agent each user profile as an `inetOrgPerson` entry, the
printers agent each printer with an output queue (docs/DIRSRV.md
§10.1). They run as jobs `QGLDPUBE` and `QGLDPUBA` in `QSYSWRK`.
Configure each on Navigator's Directory server > Publishing tab: whether
it publishes, the directory server (blank for this system's default
instance), its port and TLS, the DN it binds as and its password, and
the parent DN of the entries. The configuration is kept in
`/QIBM/UserData/OS400/DirSrv/publish.conf`. Until publishing is
complete the agents end at once with `GLD010B`.

## The QSH tools

In QSH (`STRQSH`) the LDAP tools are `ldapsearch`, `ldapadd`,
`ldapmodify`, `ldapdelete`, `ldapmodrdn`, `ldapchangepwd`, `ldapexop`,
`ldapcompare` and `ldapdiff`, with the usual option letters (`-h` host,
`-p` port, `-D` bind DN, `-w` password, `-b` base, `-s` scope, `-Z` TLS).
For example:

```
ldapsearch -h localhost -D "cn=Administrator" -w secret \
  -b "dc=example,dc=com" -s sub "(uid=jsmith)" cn mail
ldapmodify -D "cn=Administrator" -w secret -f changes.ldif
```

`LDAP_BASEDN` in the environment gives `ldapsearch` its base when `-b`
is left out. `ldapdiff` compares the same subtree on two servers.

## The client API

ILE programs bind to service program `QSYS/QGLDCLNT`, the LDAP C API:
`ldap_init` or `ldap_ssl_init`, `ldap_simple_bind_s`, `ldap_search_s`,
`ldap_first_entry` and `ldap_next_entry`, `ldap_get_values`,
`ldap_modify_s`, `ldap_add_s`, `ldap_delete_s`, `ldap_unbind` and the
rest. Include `QSYSINC/H` members `LDAP`, `LBER` and `LDAPSSL` in C;
RPG programs copy `QSYSINC/QRPGLESRC(LDAP)`. Strings are in the job's
CCSID unless the program asks for UTF-8 (`LDAP_OPT_UTF8_IO`); a TLS
connection names its certificate store application ID with
`ldap_app_ssl_*`. A function QS/400 does not provide returns
`LDAP_NOT_SUPPORTED`.

## Messages

The directory server's messages are `GLD` messages in `QSYS/QGLDMSG`
(the Knowledge Center's message finder lists them). Those you meet most:
`GLD0123` (an instance changed), `GLD0126` and `GLD0232` (a
configuration value the instance does not take), `GLD0127` (a
configuration file it cannot read), `GLD0201` and `GLD0203` (no such
instance), `GLD0202`, `GLD0215` and `GLD0224` (`CHGDIRSVRA`'s
refusals), `GLD010B` (the publishing agents).
