# Licensed programs

The `licpgm` crate (`licpgm/licpgm`) models IBM i's licensed programs
for QS/400: the products and their options, the product definition
(`*PRDDFN`) and product load (`*PRDLOD`) objects, the product
directory, the QSZ* product APIs, RSTLICPGM/DLTLICPGM/SAVLICPGM
planning, QSPKGPRD manifests, license keys and license use, and
software agreements. It is a library: the object store, `slic`, the
commands and the GO LICPGM panels reach it through the traits in
`store.rs` and `plan/mod.rs`.

IBM's names (commands, APIs, formats, status codes, option numbers)
are the interface. Every text a user sees is Quartz's own.

## 1. Product scheme

Quartz products use IBM's seven-character form with the prefix `QS`,
which no IBM or vendor ID can have (IBM's start with a digit). The
`QS` range is reserved: a QSPKGPRD manifest may not use it unless the
build pipeline asks.

| Product | Status | IBM alias | Description |
|---|---|---|---|
| `QS00SS1` | shipped | `5770SS1` | Quartz Systems/400 Operating System |
| `QS00WDS` | shipped | `5770WDS` | Quartz Systems/400 Application Development |
| `QS00TC1` | shipped | `5770TC1` | Quartz Systems/400 TCP/IP Utilities |
| `QS00ST1` | shipped | `5770ST1` | Quartz Systems/400 SQL Database Utilities |
| `QS00DG1` | reserved | `5770DG1` | Quartz Systems/400 HTTP Server |
| `QS00NAV` | reserved | | Quartz Systems/400 Navigator |
| `QS00AFW` | reserved | `5770XH2` | Quartz Systems/400 Access for Web |
| `QS00PSE` | reserved | | Quartz Systems/400 PASE Runtime |

A product answers to its IBM alias as well (`QSZRTVPR` with `5770SS1`
finds `QS00SS1`), so software that checks for IBM's product finds
ours. Reserved products are in the catalog with their IDs, options and
agreements, never installed.

- **Release**: every shipped product is at the system's release, from
  the `release` crate (`V1R0M0`).
- **Options**: `*BASE` (API form `0000`) and 1..99 (`0001`..`0099`).
- **Language feature**: `2924` (English), the only language shipped.
- **Load types**: `*CODE` (the programs) and `*LNG` (one language's
  objects). Each option has one code load and one language load per
  language.
- **Load IDs**: a code load's ID is its license feature, 5001..9999;
  a language load's ID is the language feature (`29xx`).
- **Load object names** (default): the last three characters of the
  product ID (or `P` and the last two when those start with a digit),
  the option in two digits, `C` or `L`, the load ID:
  `SS100C5050`, `SS100L2924`, `WDS31C5102`, `P3400C5001` for `1AB1234`.
  A manifest whose product ID
  gives no valid name is refused.

### QS00SS1 options

QS/400's components mapped to IBM's 5770-SS1 option numbers where IBM
has a direct analogue. All share the operating system's feature, 5050.

| Option | Description | QS/400 components | IBM |
|---|---|---|---|
| `*BASE` | Operating system | slic, the store, the database (DDS, journals, SQL), ILE (qir, codegen, mi, ile, sysapi, msgf), work management, spooling and the host print transform, security, commands and CL, TN5250 and display files, UIM, menus, EDTF, the IFS, TCP/IP configuration, sockets, TLS, DDM/DRDA, the servers framework, the RPG runtime | `*BASE`; DDM, TCP/IP, TLS and the host print transform are base there too |
| 12 | Host servers | hostsvr, hostserver, regfac | 12, Host Servers |
| 18 | Media and storage extensions | image catalogs, virtual optical (`/QOPT`), save/restore media exits | 18 |
| 30 | Qshell | qsh, qsh-utils, qsh-job, qshterm | 30, Qshell |
| 33 | Portable application environment | the PASE loader and system call layer | 33, PASE |
| 34 | Digital certificate manager | certs (needs option 12) | 34, DCM |

Absent, with no QS/400 counterpart: OptiConnect (23), the CCA
cryptographic provider (35), PSF (36-38), ICU (39), the high
availability options (41, 42), additional fonts (43), encrypted backup
and ASP enablement (44, 45), System/36 and System/38 migration (5, 6,
11). The PDF writer and network printing stay in `*BASE`; IBM's
equivalents are separate products (Transform Services, Infoprint
Server) with no SS1 option.

### QS00WDS options

| Option | Feature | Description | QS/400 components | IBM |
|---|---|---|---|---|
| `*BASE` | 5001 | Common compiler support | rpg/specs, listings, event files | `*BASE` |
| 21 | 5101 | Application development tools | SEU, PDM, SDA, DFU, RLU, the debugger screens | 21, Tools - Application Development |
| 31 | 5102 | ILE RPG compiler | rpg/front, rpg/compiler | 31, Compiler - ILE RPG |
| 61 | 5103 | SQL precompiler | rpg/sqlpre (`CRTSQLRPGI`), needs 31 | none: IBM ships the precompilers with 5770ST1 |

The CL compiler is part of the operating system, as on IBM i. There is
no COBOL, C or C++ compiler. QS00TC1 `*BASE` holds FTP, the LPD,
the SMTP client, SNTP, the SSH server and the Telnet server; QS00ST1
`*BASE` holds STRSQL. Every product's `*BASE` needs `QS00SS1 *BASE`.

### The catalog

`licpgm/licpgm/catalog.toml` lists the products as QSPKGPRD manifests
(§6) inside `[[package]]` tables, with two catalog-only fields:
`product.status` (`shipped` or `reserved`) and per option the notes
`components` and `ibm`. `catalog::shipped()` parses it once; a unit
test keeps it valid. Agreement texts are `agreements/<ID>.txt`.

## 2. Payloads

Proposed object type codes (the store crate is not changed here):
**`*PRDDFN` = `0x002A`**, **`*PRDLOD` = `0x002B`**, both
`user_creatable` (QSZCRTPD/QSZCRTPL), not temporary, not QSYS-only,
`Q` names not reserved, payload module `Opaque`. The license
repository and the agreement answers (§7, §8) are records the
integrator keeps in an object of its choice (proposed: one `*USRSPC`
each in `QUSRSYS`, `QLZALIC` and `QSZAGREE`).

Character fields are EBCDIC (CCSID 37), blank padded; numbers are
big-endian; a release is `VxRyMz` (or `vvrrmm` past 9); dates are
`YYYYMMDD` as a `u32`, 0 for none. A payload is its header followed by
its sections in the order of the counts; nothing follows the last.
`licpgm/licpgm/tests/spec.rs` compares these tables with the
code's layouts.

### Product definition

Sections after the header: options, prerequisites, languages, CCSIDs,
licenses.

### `PrdDfnHeader` (192 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QSPRDDFN` (ASCII) |
| 8 | 2 | format_version | 1 |
| 10 | 2 | flags | bit 0 allow multiple releases, 1 allow mixed releases, 2 licensed, 3 license key present |
| 12 | 7 | product_id | |
| 19 | 6 | release | |
| 25 | 7 | ibm_alias | blank for none |
| 32 | 10 | msgf_name | blank for none |
| 42 | 10 | msgf_lib | |
| 52 | 2 | first_copyright | year, 0 for none |
| 54 | 2 | current_copyright | year, 0 for none |
| 56 | 4 | release_date | `YYYYMMDD` |
| 60 | 2 | registration_type | `02`, `04`, `08` |
| 62 | 14 | registration_value | |
| 76 | 50 | text | |
| 126 | 10 | public_aut | |
| 136 | 32 | license_key | ed25519 public key, zeros for the system's |
| 168 | 2 | option_count | |
| 170 | 2 | prereq_count | |
| 172 | 2 | language_count | |
| 174 | 2 | ccsid_count | |
| 176 | 2 | license_count | |
| 178 | 14 | reserved | zeros |

### `PrdDfnOption` (80 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | option | 0 is `*BASE` |
| 2 | 2 | flags | bit 0 dynamic naming, 1 min_base present |
| 4 | 7 | msg_id | |
| 11 | 4 | feature | code load ID |
| 15 | 6 | min_base | lowest `*BASE` release |
| 21 | 50 | text | |
| 71 | 9 | reserved | zeros |

### `PrdDfnPrereq` (24 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | option | the option that needs it |
| 2 | 7 | product_id | |
| 9 | 2 | required_option | |
| 11 | 6 | min_release | blank for any |
| 17 | 7 | reserved | zeros |

### `PrdDfnLanguage` (12 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | option | |
| 2 | 8 | load | `29xx`, `*NONE`, `*IBMLNG`, `*BASEOPT` |
| 10 | 2 | reserved | zeros |

### `PrdDfnCcsid` (4 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | ccsid | |

### `PrdDfnLicense` (32 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 4 | feature | |
| 4 | 6 | term | `V1`, `V1R0`, `V1R0M0` |
| 10 | 1 | usage_type | 0 `*CONCURRENT`, 1 `*REGISTERED`, 2 `*PROCESSOR` |
| 11 | 1 | compliance | 0 `*OPRACTION`, 1 `*WARNING`, 2 `*KEYED` |
| 12 | 4 | usage_limit | `0xFFFFFFFF` is `*NOMAX` |
| 16 | 4 | threshold | 0 for none |
| 20 | 2 | grace_days | |
| 22 | 10 | reserved | zeros |

### Product load

Sections after the header: libraries (the principal first), preoperation
exit programs, folders, directories, objects, paths, agreements.

### `PrdLodHeader` (160 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QSPRDLOD` (ASCII) |
| 8 | 2 | format_version | 1 |
| 10 | 2 | flags | bit 0 supported, 1 own registration (else `*PRDDFN`) |
| 12 | 7 | product_id | |
| 19 | 6 | release | |
| 25 | 2 | option | |
| 27 | 1 | load_type | 0 `*CODE`, 1 `*LNG` |
| 28 | 4 | load_id | |
| 32 | 2 | state | `10`..`90` (§3) |
| 34 | 2 | registration_type | blank unless flag 1 |
| 36 | 14 | registration_value | |
| 50 | 6 | min_target | blank for the current release |
| 56 | 10 | secondary_lib | blank for none |
| 66 | 50 | text | |
| 116 | 10 | public_aut | |
| 126 | 2 | library_count | |
| 128 | 8 | installed_at | seconds since the epoch, 0 if not |
| 136 | 2 | pre_exit_count | |
| 138 | 2 | folder_count | |
| 140 | 2 | directory_count | |
| 142 | 2 | agreement_count | |
| 144 | 4 | object_count | |
| 148 | 4 | path_count | |
| 152 | 8 | reserved | zeros |

### `PrdLodLibrary` (72 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | dev | development library |
| 10 | 10 | primary | name it installs under |
| 20 | 10 | installed | name it was installed under |
| 30 | 10 | post_exit | postoperation exit program, blank for none |
| 40 | 10 | lib_type | `*PROD`, `*TEST` |
| 50 | 10 | aut | |
| 60 | 10 | crt_aut | |
| 70 | 2 | reserved | zeros |

### `PrdLodExit` (20 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | program | |
| 10 | 10 | library | |

### `PrdLodFolder` (192 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 63 | dev | |
| 63 | 63 | primary | |
| 126 | 63 | installed | |
| 189 | 3 | reserved | zeros |

### `PrdLodDirectory` (variable)

The four names are UTF-8, back to back, then the authorities.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | primary_len | |
| 2 | 2 | home_len | |
| 4 | 2 | installed_len | |
| 6 | 2 | installed_home_len | |
| 8 | 10 | data_aut | `*RWX` ... `*EXCLUDE` |
| 18 | 2 | obj_aut_count | |
| 20 | primary_len | primary | |
| var | home_len | home | |
| var | installed_len | installed | |
| var | installed_home_len | installed_home | |
| var | 10 x count | obj_auts | `*OBJEXIST`, ... |

### `PrdLodObject` (32 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | library | primary name |
| 10 | 10 | name | or `*ALL` |
| 20 | 10 | type | or `*ALL` |
| 30 | 2 | reserved | zeros |

### `PrdLodText` (variable)

A path or an agreement document path.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 2 | len | |
| 2 | len | text | UTF-8 |

## 3. The product directory

Installed state lives where IBM keeps it: in each `*PRDLOD`. The
directory module computes the views from the store.

**Load states** (QSZRTVPR's two-character code, symbolic state, error
indicator):

| Code | Meaning | Symbolic | Error |
|---|---|---|---|
| 10 | in the definition, no load object | `*DEFINED` | |
| 20 | load object created | `*CREATED` | |
| 30 | packaged | `*PACKAGED` | |
| 32 | packaged, a library or folder renamed | `*PACKAGED` | `*ERROR` |
| 33 | packaged, an object damaged | `*DAMAGED` | `*ERROR` |
| 34 | packaged, an object missing | `*PACKAGED` | `*ERROR` |
| 35 | RSTLICPGM running, was packaged | `*PACKAGED` | `*ERROR` |
| 38 | DLTLICPGM running, was packaged | `*PACKAGED` | `*ERROR` |
| 3E | RSTLICPGM stopped by a preoperation exit, was packaged | `*PACKAGED` | `*ERROR` |
| 3F | RSTLICPGM failed, was packaged | `*PACKAGED` | `*ERROR` |
| 50 | RSTLICPGM running, was installed | `*LOADED` | `*ERROR` |
| 53 | DLTLICPGM running, was installed | `*LOADED` | `*ERROR` |
| 59 | too old for this release | `*PACKAGED` | `*ERROR` |
| 60 | restored, not yet checked | `*LOADED` | |
| 61 | restored, the postoperation exit failed | `*LOADED` | `*ERROR` |
| 62 | installed, a library or folder renamed | `*LOADED` | `*ERROR` |
| 63 | installed, an object damaged | `*DAMAGED` | `*ERROR` |
| 64 | installed, an object missing (or DLTLICPGM failed) | `*LOADED` | `*ERROR` |
| 67 | CHKPRDOPT's exit program failed | `*LOADED` | `*ERROR` |
| 6E | RSTLICPGM stopped by a preoperation exit, was installed | `*LOADED` | `*ERROR` |
| 6F | RSTLICPGM failed, was installed | `*LOADED` | `*ERROR` |
| 90 | installed | `*INSTALLED` | |

Codes `5x`, `6x` and `90` count as installed on this system. The
symbolic names for the in-progress codes and the error indicator of
`35`/`38`/`50`/`53` are this implementation's reading.

**Installed status** of an option (GO LICPGM option 10), first match:
`*DAMAGED` (a load at 63), `*ERROR` (a load not at 90, or an option
without its `*BASE`), `*NOPRIMARY` (the option has language loads but
not the system's primary language), `*BKLVLOPT` / `*BKLVLBASE`
(requirements between option and `*BASE` not met, below), then for
Quartz products `*COMPATIBLE` (same V and R as the system),
`*BACKLEVEL` (older), `*INSTALLED` (newer); a non-Quartz product is
`*INSTALLED` (its compatibility is not known).

**Requirements met between base and option** (QSZRTVPR): `0` for
language loads or no `*BASE` installed; `3` when `*BASE` is older than
the option's minimum `*BASE` release; with mixed releases not allowed,
`2` when the option is older than `*BASE` and `3` when newer; else `1`.

**DSPSFWRSC** rows: one per installed load: resource ID (product),
option, feature (load ID), load type, release, description. **GO
LICPGM 10** rows: one per installed option: product, option, release,
installed status, description.

**CHKPRDOPT** checks every installed load of the product (or one
option, or one release): each object in the load's object list, in its
installed library (`*ALL` entries expand to the library's objects), is
present, not damaged and, when the object's description records a
product release, at the load's release; each IFS path is present and
not damaged; each prerequisite option is installed. Then: damage moves
the load to 63 (33 if packaged), anything missing or at the wrong
release to 64 (34), and a clean check moves 60, 63 and 64 back to 90
(33 and 34 back to 30).

## 4. The APIs

`api/` takes each parameter's bytes and returns the receiver or an
`ApiError` (message ID and replacement data) for the error code
parameter. The integrator installs them as natives of `QSYS` the way
`sysapi` does: read the parameters with `sysapi::param::Args`, call,
`Out::deliver` the bytes, `ErrCode::fail` the error.

| API | Function | Parameters read |
|---|---|---|
| `QSZRTVPR` | `api::rtvpr::qszrtvpr` | receiver length, format, product information, its format (`PRDI0100`, `PRDI0200`) |
| `QSZCRTPD` | `api::crtpd::qszcrtpd` | qualified name, definition information (106), option list (41 each), count, language load list (20 each), count, text, public authority |
| `QSZDLTPD` | `api::crtpd::qszdltpd` | qualified name |
| `QSZCRTPL` | `api::crtpl::qszcrtpl` | name, load information (87), secondary language library, principal library (30), additional libraries (30 each), count, preoperation exits (20 each), count, folders (126 each), count, text, public authority, number of directories |
| `QSZDLTPL` | `api::crtpl::qszdltpl` | qualified name |
| `QSZPKGPO` | `api::pkgpo::qszpkgpo` | option information (35), repackage, allow object change |
| `QSZSPTPR` | `api::sptpr::qszsptpr` | product information (`SPTP0100`), length, format, action |
| `QSZCHKTG` | `api::chktg::qszchktg` | target release, supported releases, count |
| `QLZAREQ` | `api::lic::qlzareq` | `LICP0100`, its format, license user, its format (`LICL0100`, `LICL0200`) |
| `QLZARLS` | `api::lic::qlzarls` | the same |

Typed equivalents are there for callers that are not APIs:
`crtpd::create_definition`, `crtpl::create_load`, `pkgpo::package`
(PKGPRDOPT), `LicenseRepo::request` and `release`.

### QSZRTVPR

Every format but `PRDR0700` starts with `PRDR0100`; their own fields
start at the "offset to additional information", which QS/400 always
sets to 108 (the end of `PRDR0100`; IBM documents the offset, not its
value). Offsets below are decimal.

`PRDR0100`: 0 bytes returned, 4 bytes available, 8 reserved (`bin4`
0), 12 product ID (7: the ID asked for, so an alias is echoed), 19
release (6), 25 option (4), 29 load ID (4), 33 load type (10), 43
symbolic load state (10), 53 load error indicator (10), 63 load state
(2), 65 supported flag (1), 66 registration type (2), 68 registration
value (14), 82 reserved (2), 84 offset to additional information
(`bin4`, 0 for `PRDR0100`), 88 primary language load identifier (4),
92 minimum target release (6), 98 minimum VRM of `*BASE` required by
the option (6), 104 requirements met (1), 105 level (3, `L00` for the
operating system, blank otherwise).

`PRDR0200` at 108: secondary language library (10), reserved (2),
number of libraries (120, `bin4`), offset to the first library record
(124, `bin4`); records from 128, each: offset to the next record
(`bin4`, from the start of the receiver, 0 on the last), primary
library, installed library, library type, library authority, library
create authority, postoperation exit program (`*NONE` if none), each
10, number of preoperation exit programs (`bin4`, at 64), their names
(10 each); padded to a multiple of 4. The load's preoperation exits are
listed in the principal library's record.

`PRDR0300` at 108: number of folders, length of one record (126),
offset (120); records of primary folder (63) and installed folder (63).

`PRDR0400` at 108: number of objects, length of one record (30),
offset (120); records of object name, installed library, object type,
each 10.

`PRDR0500` at 108: allow multiple releases (1), release date century
(1: `0` 19xx, `1` 20xx, blank none), release date (6, `yymmdd`), first
copyright (4), current copyright (4), message file (10), its library
(10), number of options (144), length of one option record (148, 18),
offset (152, 160), allow mixed releases (156, 1), reserved (3); records
from 160: option (4), allow dynamic naming (1), message ID (7), minimum
VRM of the option (6).

`PRDR0600` at 108: number of loads, length of one record (8), offset
(120); records of option (4) and load ID (4), for every load the
definition names (the code load and each language of every option).

`PRDR0700`: bytes returned, bytes available, number of releases (8),
reserved (12, `bin4`), releases from 16 (6 each, ascending): the
releases of the product that have a definition on the system.

`PRDR0800` at 108: number of directories, length of one entry (52),
offset (128), CCSID of the names (120), conversion error indicator
(124, `0`/`1`), reserved (3); entries from 128: length and offset of
the primary path, primary home directory, installed path and installed
home directory (`bin4` pairs), number and offset of the public object
authorities, public data authority (10), reserved (2); the names and
the authorities (10 each) follow the entries. Names are given in CCSID
37 (0 or 37), 1208, 1200 or 13488; another CCSID gives UTF-16 (1200)
with the error indicator set.

Fields this reading is unsure of, to settle on an IBM i system:
the value of the "offset to additional information"; PRDR0200's
"offset to next library record" base (receiver start assumed) and
where the preoperation exit names belong; PRDR0500's option record
length (18 here, the fields with no reserved tail); what PRDR0700's
list holds (IBM's text names release levels; QS/400 lists the
product's releases on the system); PRDR0800's entry length and the
place of the data; the registration value given for type `04` (the
operating system's, else the serial number); the primary language
field for a product with no language load installed (blank). The
message IDs `CPF0C1C` (several releases), `CPF0C1D` (release not
valid) and `CPF0C1F` (no such product, option or load) are chosen from
IBM's list for QSZRTVPR without their texts to confirm them.

### The others

- **QSZCRTPD** checks as IBM documents: the first option is `*BASE`
  (`CPF0CAA`), options unique (`CPF0CAF`), 1..100 of them (`CPF0CBA`),
  1..139 language loads and at most 40 for `*BASE` (`CPF0CB9`),
  `*IBMLNG` only for `*BASE` and `*BASEOPT` only for others
  (`CPF0CAB`), code load IDs 5001..9999 (`CPF0CBD`), copyrights in
  order (`CPF0CB0`), reserved fields blank (`CPF0CB3`), and refuses a
  second definition of a product and release (`CPF0C17`). The option
  text is not in the parameters (IBM takes it from the message file);
  displays fall back to the definition's text.
- **QSZCRTPL** creates the load in its principal development library
  (`*PRDDFN`: the definition's library); the load ID must be the
  option's feature (`*CODEDFT` is 5001) or one of its languages
  (`CPF0C84`). The directory list (optional group 1) is not read: its
  format `SDIR0100` is not documented to us; a count other than 0 is
  `CPF3C21` until it is (`MIGRATE(phase11)`), and `create_load` takes
  directories typed. The software agreement list (group 2) likewise.
- **QSZPKGPO** packages the loads (state 30) after checking every
  listed object is present and undamaged (`CPF9801`, `CPF0C19`). A
  code load with no object list takes every object of its development
  libraries. `*SAME`/`*NO` only concern the objects' descriptions.
- **QSZDLTPD** refuses a definition whose product still has loads at
  that release, **QSZDLTPL** an installed load (`CPF0C52`).
- **QSZSPTPR** sets or clears a load's supported flag and, when given,
  its registration.
- **QSZCHKTG** resolves `*CURRENT` and `*PRV` (the newest known
  release with modification 0 below the current version and release)
  and finds the newest supported release not newer than the target.
  QS/400 knows only its own releases, so `TGTRLS(V7R1M0)` is not a
  known release (`CPF0C34`). IBM's rule that V1R3M0 is the earliest
  supported entry does not apply to Quartz releases.
- **QLZAREQ / QLZARLS**: §7.

## 5. Install, delete, save

`plan/` decides; the integrator's traits do: `Media` lists the loads on
a medium and yields each part's save stream (`Part::Library` by
development name, `Part::Directory` by primary path); `Restorer`
restores a stream into a library or under a path (the save engine);
`ExitRunner` calls exit programs; `Remover` deletes for DLTLICPGM.

**RSTLICPGM** (`plan_restore`, then `execute_restore`). Per request
(`LICPGM`, `OPTION`, `RLS`, `LNG`, `RSTOBJ`): the release is the one
given or the only one on the media; `*ALL` and `*PGM` need the code
load on the media; `*ALL` and `*LNG` need the language load of `LNG`
(default the system's primary language) when the option has languages;
`*LNG` needs the code installed. An option needs its `*BASE` installed
or in the plan, and every prerequisite installed (at its minimum
release) or in the plan. Order: a load comes after everything it
depends on in the plan (its code load, its `*BASE`, its
prerequisites), ties broken by product, option, `*CODE` before `*LNG`.
A load replaces the installed one of the same product, option, type
and ID at the same release, or at any release when the product does
not allow multiple releases. Products whose base code load lists
agreement documents and whose agreement has not been accepted are
listed; `execute_restore` refuses until they are. Carrying out one
load: the definition is created if missing; the load goes to 35 (or 50
when it replaces an installed load); preoperation exits run (failure
3E/6E); each library and directory is restored (failure 3F/6F); 60;
postoperation exits run (failure 61); 90 with the time installed. A
load with no object list takes the objects restored. The product's
license information is created from its definition.

**DLTLICPGM** (`plan_delete`, `execute_delete`). `OPTION(*ALL)` or
`*BASE` with no `LNG` takes every option of the product; an `LNG`
takes only that language load. Refused when installed options of other
products (or options staying) need a code load that would go. Loads
go language first, options before `*BASE`. A library goes whole unless
it is a system library (`QSYS`, `QSYS2`, `QUSRSYS`, `QGPL`, `QTEMP`,
`QHLPSYS`, `QSPL`) or another remaining load uses it; otherwise the
load's objects go one by one. Directories likewise (never `/`,
`/QOpenSys`, `/QIBM`, `/QIBM/ProdData`, `/QIBM/UserData`, `/QSYS.LIB`,
`/home`, `/tmp`). A definition with no load left goes, with its
agreement answer and license information; license keys stay. A
failure leaves the loads at 64.

**SAVLICPGM** (`plan_save`): the loads selected (`OPTION`, `RLS`,
`LNG`, `OBJTYPE`) must be at 90 or 30; each brings its libraries whole,
or from system libraries its objects one by one, its directories, and
the product's `*PRDDFN`.

## 6. QSPKGPRD manifests

A product to package is described by a TOML file in the IFS.
`pkg::parse` checks it and builds the definition and the loads;
`Package::definition_payload` and `load_payloads` give the object
payloads. Every problem is reported with where it is
(`option[2].objects[0].lib`) and what is wrong, all at once.

```toml
[product]
id = "1AB1234"            # nxxxxxx; QS..... is Quartz's
release = "V1R0M0"         # default: the system's
text = "Inventory control"
library = "INVPRD"         # *BASE's principal library, where the *PRDDFN goes
object = "INVPRD"          # *PRDDFN name; default the product ID (P1AB1234)
msgf = "INVPRD/INVMSG"
first_copyright = 2024
current_copyright = 2026
release_date = "2026-10-30"
allow_multiple_releases = false
allow_mixed_releases = false
registration_type = "*PHONE"   # *PHONE, *CUSTOMER, *OPSYS (default)
registration_value = "15551234"
ccsids = [37]
languages = ["2924"]       # *BASE's; default ["2924"]
license_key = "64 hex digits"  # the vendor's ed25519 public key
public_aut = "*USE"

[[option]]
option = "*BASE"           # first; then 1..99
text = "Inventory control base"
msgid = "INV0001"
feature = "5001"           # default 5001
dynamic_naming = false
min_base = "V1R0M0"        # options only
requires = ["QS00SS1:*BASE", "QS00SS1:33:V1R0M0"]
languages = ["2924"]       # default: *BASE's
libraries = [{ name = "INVDEV", primary = "INVPRD", post_exit = "INVPOST", type = "*PROD", aut = "*USE", crt_aut = "*SYSVAL" }]
pre_exits = ["INVPRD/INVPRE"]
objects = [{ lib = "INVPRD", name = "*ALL", type = "*PGM" }]
lng_objects = [{ name = "INVMSG", type = "*MSGF" }]
lng_library = "INVPRD2924"
directories = [{ path = "/QIBM/ProdData/INV", data_aut = "*RX", obj_auts = [] }]
paths = ["/QIBM/ProdData/INV/bin"]

[[license]]
feature = "5001"
term = "V1"                # default the release's version
usage_type = "*CONCURRENT" # *REGISTERED, *PROCESSOR
compliance = "*KEYED"      # *OPRACTION, *WARNING (default)
usage_limit = 25           # or "*NOMAX"
grace_days = 30
threshold = 20

[agreement]
text = "..."               # or document = "/path/in/the/ifs"
```

Checks: the product ID's form and the `QS` reservation; names of
objects and libraries; texts of at most 50 characters the system can
store; releases and dates; `*BASE` first, options unique, at most 100;
at most 40 languages for `*BASE` and 139 language loads in all;
features 5001..9999; prerequisites `PRODUCT:OPTION[:VxRyMz]`; objects
in one of their option's libraries; directories absolute; paths below
one of their option's directories; authorities among IBM's values;
license features belonging to an option; `*KEYED` needing a
`license_key` (except Quartz products, checked with the system's key);
usage limits 0..999999 or `*NOMAX`; an agreement as text or a
document, not both. Text agreements are written by the integrator to
`/QIBM/ProdData/LicenseDoc/<ID>_<release>/2924.txt`, the path the base
code load lists. The catalog's `status`, `components` and `ibm` fields
are refused outside the catalog (the latter two are accepted and
ignored).

## 7. License keys and use

**Design.** An ed25519 signature (the same scheme and crate as the
build's manifest signature) over the key's fields cannot fit IBM's 18
characters, so a QS/400 key is 108 characters printed in IBM's style
six times over: six lines of three 6-character groups, entered as
`LICKEY` with 18 elements instead of IBM's 3. The key carries only the
signature and a check; the fields it is bound to are ADDLICKEY's own
parameters. `LICKEYINP(*LICKEYFILE)` reads them all from a key file,
the way keys are normally delivered (`docs/SVC_OPEN_QUESTIONS.md` 1).

- **Signed message**: `QS400-LICKEY-1\0`, then at fixed widths (ASCII,
  blank padded): product (7), term (6), feature (4), serial number (8),
  processor group (4), usage limit (`u32`, `0xFFFFFFFF` `*NOMAX`),
  expiration (`u32` `YYYYMMDD`, 0 `*NONE`), vendor data (8).
- **Characters**: the 512 signature bits, then a 28-bit check (the
  first 28 bits of SHA-256 of `QSLK` and the signature), five bits per
  character in the alphabet `0123456789ABCDEFGHJKMNPQRSTVWXYZ`. Input
  ignores spaces and `-`, takes either case, and reads `I`/`L` as `1`
  and `O` as `0`. A mistyped key fails the check (a typing error) before
  the signature is tried (a key for other fields).
- **Public key**: the product definition's `license_key`, else the
  system's Quartz key: the build's signing key, `/qs400/keys/system.pub`
  in the image (§9; no key is embedded in this crate).
- **Generation**: `license::key::generate(signing_key, fields)` for the
  build pipeline and tests.

**Key file** (TOML):

```toml
[[key]]
product = "QS00WDS"
term = "V1"
feature = "5102"
serial = "QS40001"
processor_group = "*ANY"
usage_limit = 10           # or "*NOMAX"
expiration = "2027-12-31"  # or "*NONE"
vendor_data = ""
key = """
XXXXXX XXXXXX XXXXXX
... six lines ...
"""
```

**ADDLICKEY** (`LicenseRepo::add_key`): `SERIAL(*LOCAL)` is the
system's; the product, term and feature must have license terms in a
definition; the serial and processor group (`*ANY` matches all) must
be this system's; the key must not have expired and must verify. A
key replaces the one with the same product, term, feature, serial and
processor group. **RMVLICKEY** removes the keys matching
(`remove_keys`), **DSPLICKEY** lists them with their six key lines
(`key_rows`).

**License information** per product, term and feature (from the
definition's license terms when the product is installed or a key is
added): usage type, compliance type, usage limit, threshold, grace
period, the message queue (`QSYSOPR`) and log flag, peak use and the
current users. **WRKLICINF** rows (`info_rows`) show the effective
usage limit: a keyed feature's is the highest of its valid keys' (0
with none, `*NOMAX` if any key says so); others' is their own.
**CHGLICINF** (`change_info`) changes the limit (not for `*KEYED`), the
threshold, the message queue and the log flag.

**QLZAREQ** (`LicenseRepo::request`). The user is a name of 1..80
characters (`*JOB` becomes the job's name); uses 1..999999. A user
already holding the same uses is granted again with nothing changed; a
different number is `CPF9E79`. Then by compliance type:

- `*KEYED`: keys exist but none is valid (expired) is `CPF9E73`. With
  no valid key, or past the keyed limit, a grace period starts (if not
  running) and requests are granted until it ends, with `CPI9E75` (no
  key) or `CPF9E72` (past the limit) as notices; after it ends they
  are refused (`CPF9E71`). Within the limit the grace period is
  cleared; a key expiring within 30 days adds `CPI9E76`.
- `*WARNING`: past the limit, granted with `CPF9E17`.
- `*OPRACTION`: past the limit, refused (`CPF9E18`).

Reaching the threshold adds a threshold notice for the message queue
(`CPI9E19` assumed). **QLZARLS** (`release`) needs the user and the
handle the request gave (`CPF9E91` otherwise). The task named a
`QLZARCAPI`; IBM has no such API: QLZAREQ requests and QLZARLS
releases (QLZARTV retrieves license information, not implemented as a
format here; `info_for` gives the data).

**License repository record** (`LicenseRepo::encode`): `QSLICRP1`, a
`u32` key count, per key product (7), term (6), feature (4), serial
(8), processor group (4), usage limit (4), expiration (4), vendor data
(8), date added (4), signature (64); a `u32` information count, per
entry product (7), term (6), feature (4), usage type (1), compliance
(1), usage limit (4), threshold (4, `0xFFFFFFFF` none), grace days (2),
grace start (4), message queue and library (10 + 10), log (1), peak (4),
a `u32` user count, per user the name (`u16` length + UTF-8), handle
(8), uses (4).

## 8. Software agreements

Each product has a short Quartz-written agreement
(`agreements/<ID>.txt`): the MIT license terms in plain words, no
warranty, and why other vendors' product names appear. The answer per
product and release (accepted or declined, by whom, when) is in
`agree::AgreementBook`: WRKSFWAGR rows (`*ACCEPTED`, `*DECLINED`,
`*NONE`), the install flow's `require`, DLTLICPGM's `forget`. Record
(`encode`): `QSAGREE1`, a `u32` count, per answer product (7), release
(6), kind (1: 1 accepted, 2 declined), date (4), user (`u16` length +
UTF-8).

## 9. In the system

- **Objects.** `*PRDDFN` is type `0x002A` and `*PRDLOD` `0x002B`
  (`storage/store/src/obj/types.rs`); each holds its payload the way
  the work management objects do (a `u32` length, then the bytes).
  `licpgm/lpstore` implements `ProductStore` and `ObjectCheck` over the
  store, one transaction per call: the license repository is the user
  space `QUSRSYS/QLZALIC`, the agreement answers `QUSRSYS/QSZAGREE`.
- **Every IPL** the service seeds the catalog (`lpstore::seed`, from
  `lic/slic/src/stgsvc/licpgm.rs`): every product's `*PRDDFN` in
  `QSYS`, the shipped products' loads there at 90, their license
  information; reserved products get their definition only. What
  exists is left alone.
- **The key.** The build writes the public half of its signing key
  (`QS400_SIGNING_KEY`, or the dev key under `target/keys/`) into the
  initramfs as `/qs400/keys/system.pub`, 32 raw bytes. The save engine
  trusts object signatures with it and ADDLICKEY checks Quartz license
  keys with its first key.
- **Commands** (`lic/slic/src/cpp/lpcmd.rs`): RSTLICPGM (`DEV(*SAVF)`,
  or an install device such as `OPT01` holding install media),
  DLTLICPGM, SAVLICPGM (a `SaveKind::LicPgm` stream into a save file),
  CHKPRDOPT, DSPSFWRSC, WRKLICINF, ADDLICKEY (`LICKEY` of 18 elements,
  or `LICKEYINP(*LICKEYFILE) LICKEYFILE(path)`, a stream file),
  RMVLICKEY, DSPLICKEY (`OUTPUT(*LICKEYFILE)` writes a key file),
  CHGLICINF, WRKSFWAGR, and Quartz's `QSPKGPRD MANIFEST(path)
  [SAVF(lib/file)]`. RSTLICPGM records the agreement as accepted by the
  user running it; GO LICPGM shows it first. The restore goes through
  `lpstore::SaveRestorer` (the save engine); the save stream holds the
  product objects, which the restorer leaves to `execute_restore`.
  Exit programs are not called yet (`NoExits`).
- **APIs.** `runtime/sysapi/src/lp.rs` installs QSZRTVPR, QSZCRTPD,
  QSZCRTPL, QSZPKGPO, QSZDLTPD, QSZDLTPL, QSZSPTPR, QSZCHKTG, QLZAREQ
  and QLZARLS as natives; each reads its parameters' bytes and asks the
  provider (`Provider::licpgm`), which in a job is the service
  (`licpgm_api`, request 1321, answered by `licpgm::api::dispatch`)
  and elsewhere the shipped catalog. `*OPSYS` is `QS00SS1`.
- **The LP matrix.** CRTBNDRPG and CRTRPGMOD need `QS00WDS` option 31
  installed, CRTSQLRPGI options 31 and 61; otherwise they end with
  `CPF0C1F`. After DLTLICPGM, RSTLICPGM from a SAVLICPGM save file
  brings them back.
- **GO LICPGM** (`ui/menus/sys/LICPGM.pnlgrp`, option 5 is
  `LICPREP`): options 1 and 10 to 22 are `QSLICPGM OPTION(n)`, panels of
  `QSZLICPGM` driven by `work/qsjob/src/panels/licpgm.rs` (data through
  `licpgm_panel`, request 1320); 50 is DSPLOG, 70 the command menu.
  Install takes the install options (the save file, the objects), the
  list of what it holds, each product's agreement (F14 accepts, F15
  declines), then RSTLICPGM per option, base first.

## 10. Product loads at build time

`build lp` (`tools/build/src/image/lp.rs`, the work in
`licpgm/lpbuild`) packages each manifest of `licpgm/products/` into
`target/images/lp/<ID>.savf`, the bytes a save file holds for
`RSTLICPGM DEV(*SAVF)`: a store in memory with `QSYS` and the system
libraries; for `QS00SS1` the objects `lic/osobjects` makes (message
files, work management objects, display files, panel groups and menus,
the registration facility, the commands, `QSTRUP`; user profiles,
system values, network attributes and the TCP/IP configuration stay
with the first IPL), which the base code load lists; the `*PRDDFN` and
packaged `*PRDLOD`s; every object signed (ed25519 over its content
digest, key ID the first 8 bytes of the public key's SHA-256, the
descriptor flagged `SIGNED`); then SAVLICPGM's stream. The digests do
not depend on the system, so two builds give the same digests. The
manifests match `catalog.toml` (a test keeps them equal). `build iso`
puts the loads on the media under `QS400/LP/`, and the operating
system install restores `QS00SS1` from there (docs/INSTALL.md); the
first IPL makes the OS objects itself only on a system installed
without media.
