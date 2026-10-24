# Service program captures

From pub400 (IBM i 7.5), 2026-10-24, over JDBC:

- `qusapibd.txt`: the entries of binding directory `QSYS/QUSAPIBD`
  (`QSYS2.BINDING_DIRECTORY_INFO`).
- `exports.txt`: `program|symbol|usage` for the service programs that
  hold the IFS, sockets, resolver, TLS, spawn, PASE and registration
  facility procedures (`QSYS2.PROGRAM_EXPORT_IMPORT_INFO`).
