# The pub400 save round trip

Captured on 2026-10-31 (UTC; 2026-10-30 here) from pub400.com (IBM i
V7R5M0) over JDBC with `QCMDEXC`, with the tool in `../tool/` (scripts
`p75-*.s`). The account is written `{user}`, its library `{user}B`, the
serial number `{serial}`.

| File | What |
|---|---|
| `survey.txt` | the account library before: 60 objects (the 2026-10-22 kit, with journal `QSTJRN` and three receivers); 235 spooled files |
| `roundtrip.txt` | `CRTSAVF`, `SAVLIB LIB({user}B) OMITOBJ(ZZ* QSSAVF)`: `CPF7080` for the attached receiver, `CPC3722 61 objects saved`; `DSPSAVF OUTPUT(*PRINT)` (header, objects, members, summary); `RSTLIB` refused to the profile (`CPD0032` "Not authorized to command RSTLIB"); the save file deleted (`CPC2191`) |
| `cleanup.txt` | the session's spooled files deleted (`QPJOBLOG` of each telnet session, the tool's compile listings), the tool's `ZZ*` objects deleted; after: 60 objects, no `ZZ*` or `QSSAVF`, 235 spooled files, none created since the session began |

The save file's panels (`DSPSAVF` list, F11 views, F16 header, option 5
on a physical and a source file, help) are goldens in
`golden/phase11/savf/`, captured between the save and the delete.
Since `RSTLIB` is refused, the restore half of the round trip has no
pub400 messages; the primary oracle's are in `../primary/`.
