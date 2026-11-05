**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; sqlfile: EMPT
//; run: no
ctl-opt dftactgrp(*no);
dcl-s n int(10);
// SET OPTION: an option that does not exist, a value that is not valid.
exec sql set option nosuch = *yes;
exec sql set option datfmt = *xyz;
exec sql set option commit = *chg, commit = *none;
exec sql select count(*) into :n from EMPT;
*inlr = *on;
