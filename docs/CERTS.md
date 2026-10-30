# Certificates and TLS

Phase 10 adds two crates under `net/`:

- `certs`: the certificate store, QS/400's equivalent of IBM's Digital
  Certificate Manager (DCM), as a library.
- `tls`: the TLS engine (rustls with the pure-Rust graviola provider)
  and the semantics of IBM's two TLS APIs, GSKit (`gsk_*`) and the
  older `SSL_*`.

Both are plain Rust. The ILE binding layer, the `QS*` commands and the
TLS servers call them; none of those is in these crates.

## Store paths

The files sit where IBM i's DCM keeps its stores, so programs that name a
keyring (`GSK_KEYRING_FILE`, `SSL_Init`) find it there. The paths are from
the primary oracle (FACTS.md: `/QIBM/UserData/ICSS/Cert/...`).

| Path | What |
|---|---|
| `/QIBM/UserData/ICSS/Cert/Server/DEFAULT.KDB` | the `*SYSTEM` store |
| `/QIBM/UserData/ICSS/Cert/Server/DEFAULT.RDB` | its stash |
| `/QIBM/UserData/ICSS/Cert/CertAuth/DEFAULT.KDB` | the local CA's store |
| `/QIBM/UserData/ICSS/Cert/CertAuth/DEFAULT.TXT` | the local CA's certificate, PEM |
| `/QIBM/UserData/ICSS/Cert/Signing/SIGNING.KDB` | `*OBJECTSIGNING` (named, unused) |
| `/QIBM/UserData/ICSS/Cert/Server/QSAPPDEF.JSON` | application definitions (Quartz) |

`*SYSTEM` given as a store name means the first path. Any other store is
named by its IFS path; its stash is the same name with `.RDB`. The files
hold the Quartz format below, not KDB: a real KDB file is refused as
damaged (`GSK_KEYFILE_INVALID_FORMAT`, `SSL_ERROR_NOT_KEYRING`).

All file access goes through `certs::files::StoreFiles` (whole-file
read, atomic replace, remove). `DirFiles` maps IFS paths under a host
directory with `std::fs`; `MemFiles` is for tests. The IFS implements the
trait for the running system.

## Store format

```text
0   magic "QS400CRT"       8
8   format_version  u32    4   = 1
12  header_len      u32    4
16  header          JSON       kind, created, Argon2id costs, salt (hex)
..  body            AES-256-GCM: nonce (12) || ciphertext || tag (16)
```

The body is the JSON of every entry, the default label and the change
time. The whole body, private keys included, is sealed under a key
Argon2id derives from the store password and the salt (default costs
19 MiB, 2 passes, 1 lane; kept in the header so they can rise later).
The clear prefix is the AEAD's associated data, so a changed header
fails to open as surely as a wrong password (`CertError::BadPassword`).

An entry is a server or client certificate with its PKCS#8 key and
intermediates, a CA certificate (trusted or not), or a request (key and
PKCS#10, waiting for its certificate). Labels are 1 to 64 characters.
The first identity added becomes the store's default certificate.

## The stash and the LIC key slot

Servers open `*SYSTEM` without a person typing its password: the password
is kept in the stash file, sealed with AES-256-GCM under a 32-byte key
held in the LIC partition's record (a key slot, docs/PHASE1.md, tag
`0x0004`, `slot_version` 1, purpose 1 = certificate stash, a generation
number bumped on rotation).

```text
0   magic "QS400STS"   8
8   version u32        4   = 1
12  generation u32     4   the key slot's generation
16  sealed             nonce || ciphertext || tag, AAD = [0, 16)
```

The sealing is behind `certs::stash::Sealer`; `KeySealer` holds the
slot's key. `stash::ensure_key_slot` gives a LIC record its slot at
first use. Rotating the key (a new generation) makes every stash
unreadable until rewritten with the store password, which is the point:
a copied disk without the LIC partition does not open the stores.

## Keys and certificates

- Keys: RSA 2048, 3072 and 4096, ECDSA P-256 and P-384 (graviola).
  Signatures: RSA PKCS#1 v1.5 with SHA-256, ECDSA with SHA-256 (P-256)
  or SHA-384 (P-384).
- Self-signed certificates (`CertStore::create_self_signed`), requests
  (`create_request`, then `receive_certificate`), and certificates the
  local CA issues (`certs::ca::issue`, `ca::sign_request`).
- The local CA is its own store with one entry,
  `LOCAL_CERTIFICATE_AUTHORITY`, holding its key; `ca::trust` adds its
  certificate to another store as a trusted CA.
- Import and export: PEM (certificates, PKCS#8, PKCS#1 and SEC1 keys,
  encrypted PKCS#8 with PBES2) and PKCS#12. PKCS#12 files are written as
  OpenSSL 3 writes them (PBES2, PBKDF2-HMAC-SHA256, AES-256-CBC, a
  SHA-256 MAC); reading also takes the older PKCS#12 PBE schemes (SHA-1
  with 3-key triple DES, RC2-40, RC2-128) and SHA-1 MACs.
- Expiry: `CertStore::expiry(now)` lists every certificate's days left;
  `expiring(now)` those past a threshold of `Expiry::THRESHOLDS`
  (30, 7, 1 days; 0 once expired). The caller sends the `QSYSOPR`
  warning the first time an entry crosses each.

## Root CAs

`net/certs/roots/cacert.pem` is the Mozilla root set as curl extracts it
from NSS. Origin, date, hash, license (MPL-2.0) and the update path are in
`net/certs/roots/README.md`. `certs::roots::seed` adds every root to a
store as a trusted CA, skipping those already present by fingerprint.

## Application IDs

`certs::apps::AppRegistry` is the list of application IDs with, for each,
its kind (server or client), the assigned certificate label in `*SYSTEM`,
the client authentication setting (none, optional, required) and the CA
trust (all trusted CAs of the store, or a list of labels). Shipped IDs
(IBM's names; doubts in docs/NET_OPEN_QUESTIONS.md, "Certificates"):

| ID | Kind |
|---|---|
| `QIBM_QTV_TELNET_SERVER` | server |
| `QIBM_QTMF_FTP_SERVER` | server |
| `QIBM_QTMF_FTP_CLIENT` | client |
| `QIBM_QTMS_SMTP_CLIENT` | client |
| `QIBM_OS400_QZBS_SVR_CENTRAL` | server |
| `QIBM_OS400_QZBS_SVR_DATABASE` | server |
| `QIBM_OS400_QZBS_SVR_DTAQ` | server |
| `QIBM_OS400_QZBS_SVR_NETPRT` | server |
| `QIBM_OS400_QZBS_SVR_RMTCMD` | server |
| `QIBM_OS400_QZBS_SVR_SIGNON` | server |
| `QIBM_OS400_QZBS_SVR_FILE` | server |
| `QIBM_QRW_SERVER_DDM_DRDA` | server |
| `QIBM_QRW_CLIENT_DDM_DRDA` | client |
| `QIBM_HTTP_SERVER_ADMIN` | server |
| `QIBM_GLD_DIRSRV_SERVER` | server |
| `QIBM_GLD_DIRSRV_CLIENT` | client |
| `QIBM_QTOB_VPN_IKE` | server |
| `QIBM_QSO_LPD_SERVER` | server |
| `QIBM_QTOC_NTP_CLIENT` | client |

A server with no certificate assigned uses the store's default. Shipped
IDs cannot be removed; users may register their own.

## TLS policy

`tls::policy::TlsPolicy::from_sysvals` reads `QSSLPCL`, `QSSLCSL` and
`QSSLCSLCTL`:

- `QSSLPCL`: `*OPSYS` is TLS 1.3 and 1.2. `*TLSV1.3` and `*TLSV1.2` are
  honored; `*TLSV1.1`, `*TLSV1`, `*SSLV3` and `*SSLV2` are accepted as
  values and reported in `unsupported_protocols`, never enabled.
- `QSSLCSL` counts only when `QSSLCSLCTL` is `*USRDFN`, as on IBM i. Its
  order is the preference order. Names not negotiable are reported in
  `unmapped_suites`.

A program or application may narrow the policy (fewer protocols or
suites) but never widen it.

### Cipher suites

| `QSSLCSL` name | IANA code | rustls |
|---|---|---|
| `*AES_128_GCM_SHA256` | 1301 | TLS13_AES_128_GCM_SHA256 |
| `*AES_256_GCM_SHA384` | 1302 | TLS13_AES_256_GCM_SHA384 |
| `*CHACHA20_POLY1305_SHA256` | 1303 | TLS13_CHACHA20_POLY1305_SHA256 |
| `*ECDHE_ECDSA_AES_128_GCM_SHA256` | C02B | yes |
| `*ECDHE_ECDSA_AES_256_GCM_SHA384` | C02C | yes |
| `*ECDHE_RSA_AES_128_GCM_SHA256` | C02F | yes |
| `*ECDHE_RSA_AES_256_GCM_SHA384` | C030 | yes |
| `*ECDHE_ECDSA_CHACHA20_POLY1305_SHA256` | CCA9 | yes |
| `*ECDHE_RSA_CHACHA20_POLY1305_SHA256` | CCA8 | yes |

These nine are `*OPSYS`, in that order. The rest are recognized (names,
codes, IANA names) and never negotiated, because rustls implements no
static-RSA key exchange, no CBC mode, no RC4, 3DES, DES, NULL or export
suites:

`*RSA_AES_128_GCM_SHA256` (009C), `*RSA_AES_256_GCM_SHA384` (009D),
`*ECDHE_ECDSA_AES_128_CBC_SHA256` (C023), `*ECDHE_ECDSA_AES_256_CBC_SHA384`
(C024), `*ECDHE_RSA_AES_128_CBC_SHA256` (C027),
`*ECDHE_RSA_AES_256_CBC_SHA384` (C028), `*RSA_AES_128_CBC_SHA256` (003C),
`*RSA_AES_128_CBC_SHA` (002F), `*RSA_AES_256_CBC_SHA256` (003D),
`*RSA_AES_256_CBC_SHA` (0035), `*ECDHE_ECDSA_3DES_EDE_CBC_SHA` (C008),
`*ECDHE_RSA_3DES_EDE_CBC_SHA` (C012), `*RSA_3DES_EDE_CBC_SHA` (000A),
`*ECDHE_ECDSA_RC4_128_SHA` (C007), `*ECDHE_RSA_RC4_128_SHA` (C011),
`*RSA_RC4_128_SHA` (0005), `*RSA_RC4_128_MD5` (0004), `*RSA_NULL_SHA256`
(003B), `*RSA_NULL_SHA` (0002), `*RSA_NULL_MD5` (0001),
`*ECDHE_ECDSA_NULL_SHA` (C006), `*ECDHE_RSA_NULL_SHA` (C010),
`*RSA_DES_CBC_SHA` (0009), `*RSA_EXPORT_RC4_40_MD5` (0003),
`*RSA_EXPORT_RC2_CBC_40_MD5` (0006).

An ECDSA suite needs an ECDSA certificate and an `ECDHE_RSA` suite an RSA
one; a server whose certificate fits none of the enabled suites fails
the handshake (`GSK_ERROR_NO_CIPHERS`).

## The engine

- `tls::config`: `Identity::from_store` (chain and key of a label,
  refused when expired), `trusted_roots`, `server_config` (client
  authentication none, optional or required against the trusted CAs),
  `client_config` (full verification with the host name, chain only as
  GSKit does by default, or passthru), and `for_app` (a configuration
  for an application ID).
- `tls::stream::TlsStream`: blocking TLS over any `Read + Write`.
- `tls::engine::Engine`: the non-blocking state machine (`feed`,
  `process`, `take_output`, `read_plain`, `write_plain`) for servers
  driving their own sockets, and `Session`: protocol, suite, peer chain,
  SNI, ALPN.

## GSKit semantics

`tls::gsk::Gsk` holds a job's environments and secure sockets and
implements each call with `gskssl.h`'s attribute IDs, enumeration values
and return codes (`tls::gsk::codes`, every value checked against
`tools/conformance/oracle/qsysinc/H/GSKSSL.txt`). `GSKVAL_OK` (0) is the
one Quartz addition: the validation code of a certificate that passed.

Order and state rules:

- Attributes of an environment are set between `gsk_environment_open`
  and `gsk_environment_init`, those of a socket between
  `gsk_secure_soc_open` (which copies the environment's) and
  `gsk_secure_soc_init`; later sets answer `GSK_INVALID_STATE`.
- `gsk_secure_soc_open` needs an initialized environment
  (`GSK_INVALID_STATE`); `gsk_secure_soc_init` needs `GSK_FD`
  (`GSK_IBMI_ERROR_BAD_SOCKET_DESCRIPTOR`); read, write and misc need a
  finished handshake (`GSK_INVALID_STATE`); an empty buffer is
  `GSK_INVALID_BUFFER_SIZE`.
- `gsk_environment_close` with sockets open: under
  `GSK_NORMAL_ENVIRONMENT_CLOSE` (the default) the handle goes at once
  and the sockets keep working until closed; under
  `GSK_DELAYED_ENVIRONMENT_CLOSE` the handle stays until its last socket
  closes and new sockets are refused with `GSK_PENDING_CLOSE_ERROR`.

Attributes honored:

| Attribute | Effect |
|---|---|
| `GSK_OS400_APPLICATION_ID` | the `*SYSTEM` store through its stash, the ID's certificate, client authentication and CA trust |
| `GSK_KEYRING_FILE`, `GSK_KEYRING_PW` | a store by path or `*SYSTEM`; the stash when no password |
| `GSK_KEYRING_LABEL`, `GSK_KEYRING_LABEL_EX` | the certificate; the store default otherwise |
| `GSK_SESSION_TYPE` | client, server, server with client authentication (optional unless `GSK_IBMI_CLIENT_AUTH_REQUIRED`), server with required client authentication |
| `GSK_CLIENT_AUTH_TYPE` | `GSK_IBMI_CLIENT_AUTH_REQUIRED` makes client authentication required |
| `GSK_SERVER_AUTH_TYPE` | full and required verify the chain; passthru accepts any server |
| `GSK_PROTOCOL_TLSV13`, `GSK_PROTOCOL_TLSV12` | `GSK_FALSE` turns the protocol off; defaults follow `QSSLPCL` |
| `GSK_PROTOCOL_TLSV11`, `_TLSV10`, `_SSLV3`, `_SSLV2`, `_TLSV1` | accepted; always off |
| `GSK_TLSV13_CIPHER_SPECS_EX`, `GSK_TLSV12_CIPHER_SPECS_EX`, `GSK_V3_CIPHER_SPECS_EX` | IANA names separated by commas (four hex digits taken too); narrows the policy |
| `GSK_V3_CIPHER_SPECS`, `GSK_TLSV12_CIPHER_SPECS` | two hex digits per suite (`0x00nn` codes) |
| `GSK_SSL_EXTN_SERVERNAME_REQUEST` | the SNI a client sends |
| `GSK_FD` | the descriptor; the binding attaches the socket with `Gsk::attach` |
| `GSK_HANDSHAKE_TIMEOUT`, `GSK_IBMI_READ_TIMEOUT` | read timeouts in seconds on the transport |
| `GSK_ENVIRONMENT_CLOSE_OPTIONS` | as above (environments only) |

Get-only: `GSK_PROTOCOL_USED`, `GSK_CONNECT_CIPHER_SPEC` (low byte, two
hex digits), `GSK_CONNECT_CIPHER_SPEC_EX` (the IANA name),
`GSK_SSL_EXTN_SERVERNAME`, `GSK_SSL_EXTN_SELECTED_ALPN_PROTOCOL`,
`GSK_CONNECT_COMPRESSION` (`NONE`), `GSK_LAST_VALIDATION_ERROR` and
`GSK_CERTIFICATE_VALIDATION_CODE` (the `GSKVAL_*` of the last handshake).
`GSK_KEYRING_PW` is never returned. Other IDs of the header are stored
and returned but change nothing (OCSP, CRL, LDAP, PKCS#11, session
cache sizes, `GSK_SSL_EXTN_ALPN`, `GSK_KEYRING_STASH_FILE`: the stash is
always the one beside the store).

Handshake failures map to return codes and validation codes: unknown
issuer `GSK_IBMI_ERROR_NOT_TRUSTED_ROOT` / `GSKVAL_ERROR_ROOT_NOT_TRUSTED`,
expired or not yet valid `GSK_ERROR_BAD_DATE`, bad signature
`GSK_ERROR_BAD_CERT_SIG`, no client certificate `GSK_ERROR_NO_CERTIFICATE`,
no common protocol or suite `GSK_ERROR_NO_CIPHERS`, a peer alert about our
certificate `GSK_ERROR_BAD_PEER`, transport `GSK_ERROR_IO`,
`GSK_ERROR_SOCKET_CLOSED` or `GSK_IBMI_ERROR_TIMED_OUT`. At
`gsk_environment_init`: an unregistered application ID
`GSK_IBMI_ERROR_NOT_REGISTERED`, a missing store `GSK_KEYRING_OPEN_ERROR`,
a wrong password `GSK_BAD_FORMAT_OR_INVALID_PASSWORD`, no usable stash
`GSK_NO_KEYFILE_PASSWORD`, an unknown label `GSK_KEY_LABEL_NOT_FOUND`, an
expired certificate `GSK_KEYFILE_CERT_EXPIRED`, a server without a
certificate `GSK_ERROR_NO_CERTIFICATE`.

`gsk_strerror` returns Quartz text for every code (`tls::gsk::strerror`).

## SSL_ semantics

`tls::ssl::Ssl` is one job's state for `qsossl.h`: `SSL_Init` or
`SSL_Init_Application` once, then handles from `SSL_Create`. The
`SSLHandle` fields a program reads (`protocol`, `cipherSuite`,
`peerCert`, `peerCertValidateRc`, ...) are `tls::ssl::SslHandle`.
`SSL_DONT_ENCRYPT` handles pass data through. Codes are
`tls::ssl::codes`, checked against `QSOSSL.txt`.

## QSOSSLSR exports

Every export of `QSOSSLSR` (tools/conformance/oracle/srvpgm/exports.txt)
and the function behind it:

| Export | Rust |
|---|---|
| `gsk_environment_open` | `Gsk::environment_open` |
| `gsk_environment_init` | `Gsk::environment_init` |
| `gsk_environment_close` | `Gsk::environment_close` |
| `gsk_attribute_set_buffer` | `Gsk::attribute_set_buffer` |
| `gsk_attribute_set_enum` | `Gsk::attribute_set_enum` |
| `gsk_attribute_set_numeric_value` | `Gsk::attribute_set_numeric_value`; for `GSK_FD` the binding also calls `Gsk::attach` with the job's socket |
| `gsk_attribute_set_callback` | `Gsk::attribute_set_callback` (refused: `GSK_API_NOT_AVAILABLE`) |
| `gsk_attribute_get_buffer` | `Gsk::attribute_get_buffer` |
| `gsk_attribute_get_enum` | `Gsk::attribute_get_enum` |
| `gsk_attribute_get_numeric_value` | `Gsk::attribute_get_numeric_value` |
| `gsk_attribute_get_cert_info` | `Gsk::attribute_get_cert_info` (`gsk::CertDataElem` array) |
| `gsk_secure_soc_open` | `Gsk::secure_soc_open` |
| `gsk_secure_soc_init` | `Gsk::secure_soc_init` |
| `gsk_secure_soc_read` | `Gsk::secure_soc_read` |
| `gsk_secure_soc_write` | `Gsk::secure_soc_write` |
| `gsk_secure_soc_misc` | `Gsk::secure_soc_misc` (`GSK_CLOSE_NOTIFY`; resets answer `GSK_ERROR_UNSUPPORTED`) |
| `gsk_secure_soc_close` | `Gsk::secure_soc_close` |
| `gsk_secure_soc_startInit` | `Gsk::secure_soc_start` (refused until the sockets crate has I/O completion ports) |
| `gsk_secure_soc_startRecv` | `Gsk::secure_soc_start` (likewise) |
| `gsk_secure_soc_startSend` | `Gsk::secure_soc_start` (likewise) |
| `gsk_strerror` | `gsk::strerror` |
| `gsk_strerror_msg` | `gsk::strerror_msg` (text; message ID and file blank) |
| `SSL_Init` | `Ssl::init` |
| `QlgSSL_Init` | `Ssl::init` (the binding converts the path first) |
| `SSL_Init_Application` | `Ssl::init_application` |
| `SSL_Create` | `Ssl::create` |
| `SSL_Handshake` | `Ssl::handshake` (the binding first copies the program's `protocol`, `timeout` and `cipherSuiteList` with `Ssl::set_before_handshake`) |
| `SSL_Read` | `Ssl::read` |
| `SSL_Write` | `Ssl::write` |
| `SSL_Destroy` | `Ssl::destroy` |
| `SSL_Strerror` | `ssl::strerror` |
| `SSL_Perror` | `ssl::perror` |

## Dependencies

All pure Rust (`cargo run -p build -- deny`): `graviola` 0.4 (keys,
signatures, AES-GCM, randomness), `rustls` 0.23 without default
features plus `tls12`, `rustls-graviola` 0.4, `rustls-webpki` 0.103,
`argon2` 0.5.3, `x509-cert` 0.3, `der` 0.8, `spki` 0.8, `signature` 3,
`pem-rfc7468` 1, and for PKCS#12 `aes` 0.9, `cipher` 0.5, `cbc` 0.2,
`des` 0.9, `rc2` 0.9, `sha1` 0.11, `hmac` 0.13, with `zeroize` 1.

## The commands

DCM is web-only on IBM i, so the commands are QS/400 additions, named
with a `QS` prefix (Quartz extensions; their messages are the `TCP1Cxx`
family of `QTCPMSG`). They run in the storage service
(`lic/slic/src/cpp/certcmd.rs`) on the library functions of
`certs::admin`, which the tests drive over `MemFiles`. Every one needs
`*ALLOBJ` and `*SECADM`, as DCM does; a refusal is `TCP1C19` and an
`AF` audit entry. `KEYSTORE` is `*SYSTEM` (the default), `*LCLCA` (the
local CA's store) or a path; `PASSWORD(*STASH)` (the default) opens the
store through its stash.

| Command | What |
|---|---|
| `QSCRTCRTS` | Create a store: `PASSWORD`, `STASH(*YES)` keeps the password sealed with the LIC key, `ROOTS(*YES)` adds the shipped root CAs. `KEYSTORE(*LCLCA)` makes the local CA (`CN`, `ORG`..., `KEYALG`, `VALIDITY`, 3650 days) and `*SYSTEM` trusts it at once; a store made later trusts an existing local CA. |
| `QSDLTCRTS` | Delete a store and its stash (and the CA's PEM copy); deleting `*SYSTEM` clears every assignment. |
| `QSCRTCRT` | A new key and certificate as `LABEL`: `SIGNER(*SELF)`, `*LCLCA` (the local CA signs; `LCLCAPWD`) or `*CSR` (a request: the key stays, the PKCS#10 PEM goes to the stream file `CSR`). `USAGE(*SERVER *CLIENT)`, `CN`, `ORG`, `ORGUNIT`, `LOCALITY`, `STATE`, `COUNTRY`, `ALTNAME` (`*CN` by default for servers), `KEYALG` (`*ECDSAP256` by default; `*ECDSAP384`, `*RSA2048`, `*RSA3072`, `*RSA4096`), `VALIDITY` (365), `TEXT`. |
| `QSIMPCRT` | Import `FROMSTMF` (read as the job): `FORMAT(*AUTO *PEM *PKCS12)`, `IMPPWD`. A certificate with its key becomes `LABEL`; CA certificates become CA entries, trusted unless `TRUST(*NO)`; a PEM certificate for a waiting request named `LABEL` completes it. |
| `QSEXPCRT` | Export to `TOSTMF` (written as the job, mode 600, CCSID 1208): `FORMAT(*PEM)` with `INCKEY(*YES)` for the key, `*PKCS12` sealed with `EXPPWD`, `*CSR` a waiting request; `REPLACE(*NO)` refuses an existing file. |
| `QSCHGCRT` | `NEWLABEL` (assignments follow), `TRUST` of a CA, `DFTCRT(*YES)` makes it the store default, `TEXT`. |
| `QSDLTCRT` | Remove a certificate; applications assigned it lose it. |
| `QSWRKCRT` | The store's certificates with type, expiry date, status (`Valid`, `Expiring` within 30 days, `Expired`, `Request`), default and assignments: the panel `QSCERT/WRKCRT` (options 2 change, 4 remove, 5 display, 7 export, 8 assign; F6 create) at a display, text otherwise. |
| `QSASNCRT` | Assign `LABEL` of `*SYSTEM` to `APPID`; `LABEL(*NONE)` takes it away. A server needs a certificate with its key. |
| `QSADDAPPID`, `QSRMVAPPID` | Register and remove an application ID (`TYPE(*SERVER *CLIENT)`); shipped IDs stay. |
| `QSCHGAPPID` | `CLTAUTH(*NONE *OPTIONAL *REQUIRED)` (servers), `CATRUST` (`*ALL` or up to 50 CA labels of `*SYSTEM`), `TEXT`. |
| `QSWRKAPPID` | The application IDs with type, certificate and client authentication: the panel `QSCERT/WRKAPPID` (2 change, 4 remove, 8 assign, 9 unassign; F6 add), text otherwise. |
| `QSCHKCRT` | The expiry scan now (below). |

The store files live in the IFS, owned by `QSYS`, mode 600, in
directories made 700. The service reads them as the system, stages the
ones a request may touch in memory, and writes back what changed, each
file replaced through a rename.

## Servers and the storage service

Only the storage service holds the LIC partition's key, so only it opens
`*SYSTEM` through its stash. A job asks for its application's
certificate with `cert_app` (`stg` request 1240): the answer is the
label, the chain, the PKCS#8 key, the client authentication setting and
the trusted CAs of the application's CA trust (`certs::admin::identity`).
The key goes only to a job running as a system server profile (`QSYS`,
`QTCP`, `QUSER`, `QTMHHTTP`, `QTMHHTP1`, `QSPLJOB`) or a user with
`*ALLOBJ` and `*IOSYSCFG`; others get `TCP1C19` and an `AF` entry. An
empty ID asks only for the trusted CAs, which any job may have. The
service keeps an answer for ten minutes while the store files are
unchanged (the stash costs an Argon2id derivation).

A server with no certificate assigned takes the store's default; with
no `*SYSTEM` store, nothing assigned and no default, an expired
certificate, or a refusal, it keeps the system's own self-signed
certificate (`QUSRSYS/QTVTLSCRT`, `QTVTLSKEY`, as before), and logs why.
A server whose user cannot create those data areas (`QTMHHTTP` for the
HTTP administration instance) makes a certificate for the run in memory.

| Server | Application ID |
|---|---|
| HTTP administration, 2005 | `QIBM_HTTP_SERVER_ADMIN` |
| Telnet, 992 | `QIBM_QTV_TELNET_SERVER` |
| Host servers (TLS ports) | `QIBM_OS400_QZBS_SVR_CENTRAL`, `_DATABASE`, `_DTAQ`, `_FILE`, `_NETPRT`, `_RMTCMD`, `_SIGNON` |
| FTP, `AUTH TLS` and 990 | `QIBM_QTMF_FTP_SERVER` |

A server reads its certificate when it starts (a prestart job keeps it
ten minutes; an FTP session job at its connection), so an assignment
takes effect at the next start. The
protocols and suites follow `QSSLPCL`, `QSSLCSL` and `QSSLCSLCTL`.

`ipps` printers (the writer's IPP connector), the `FTP` command and
`SNDSMTPEMM`/`QtmmSendMail` (when the mail router's TLS is required or
implicit) check the server's chain and name against the shipped roots
and the trusted CAs of `*SYSTEM`. A server with a self-signed
certificate is trusted by importing that certificate as a CA
(`QSIMPCRT ... TRUST(*YES)`). The FTP client presents no certificate:
the key of `QIBM_QTMF_FTP_CLIENT` goes only to server profiles.

## Expiry warnings

`QSYSCOMM1` asks the service (`cert_expiry`, request 1242) to scan ten
minutes after the IPL and then every 24 hours; `QSCHKCRT` scans at once.
The scan opens `*SYSTEM` and the local CA's store through their stashes
and looks at every certificate except the shipped roots. A certificate
crossing 30, 7 or 1 days left gets `TCP1C20` (severity 30) in `QSYSOPR`,
an expired one `TCP1C21` (40), each once: what was sent is kept in
`/QIBM/UserData/ICSS/Cert/Server/QSEXPWRN.JSON` by store and SHA-256
fingerprint, so a renewed certificate starts afresh. A scan that finds
a certificate already past several thresholds sends the nearest only.

## The LIC key slot

`slic` gives the LIC record its stash key slot when it attaches the
partition at IPL (`stash::ensure_key_slot`), so the slot exists before
the storage service starts; the service only reads it.

## Not yet

- OCSP and CRL checking; session resumption settings; ALPN from
  `GSK_SSL_EXTN_ALPN`; the asynchronous GSK calls.
- The FTP, DDM and LPD servers take their certificates the same way when
  their TLS is built.
- A store's password expiry, and the web admin UI over the same
  functions.
