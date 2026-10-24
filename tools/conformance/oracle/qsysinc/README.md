# QSYSINC captures

Source members of `QSYSINC` from pub400 (IBM i 7.5), fetched over JDBC
on 2026-10-24: the files `SYS`, `NETINET`, `ARPA` and `NET` whole, and
the members of `H` and `QRPGLESRC` that the IFS, sockets, resolver,
TLS, spawn, PASE, registration facility and exit point work of Phase 10
reads. They are IBM's text and stay under `tools/conformance/`: the
code takes structure layouts, constants and errno values from them,
never prose.
