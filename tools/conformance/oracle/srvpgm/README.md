# Service program captures

From pub400 (IBM i 7.5), 2026-10-24, over JDBC:

- `qusapibd.txt`: the entries of binding directory `QSYS/QUSAPIBD`
  (`QSYS2.BINDING_DIRECTORY_INFO`).
- `exports.txt`: `program|symbol|usage` for the service programs that
  hold the IFS, sockets, resolver, TLS, spawn, PASE and registration
  facility procedures (`QSYS2.PROGRAM_EXPORT_IMPORT_INFO`).
- The `QSQCLI` rows (the SQL CLI) were added from pub400 on 2026-11-05
  with the same view; `QSQCLI` is an entry of `QUSAPIBD` there, and no
  `QSQCLI` binding directory exists (`QSYS2.OBJECT_STATISTICS`).
