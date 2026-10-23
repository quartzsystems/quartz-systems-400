# System panel groups

One `NAME.pnlgrp` per system panel group, installed into `QSYS` as
`*PNLGRP` `NAME` when missing (`lic/slic/src/svc/ui.rs`). The job's
panel drivers (`work/qsjob/src/panels/`) open them through the storage
service and show them with the dialog engine (`docs/UIM.md` §8).

Names follow IBM's for the displays they stand behind where one is
known (`QSYS/QWCWRKAJ` for `WRKACTJOB`, ...), else `QUI` plus the
command name. Every word is Quartz-written; the geometry comes from
the goldens under `tools/conformance/golden/verified/`.
