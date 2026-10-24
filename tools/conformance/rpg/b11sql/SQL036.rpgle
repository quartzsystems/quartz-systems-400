**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; with: SQL036P
//; setup: CRTSQLRPGI OBJ({lib}/SQL036P) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE)
//; cleanup: DLTPGM PGM({lib}/SQL036P)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no) actgrp(*new);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr child extpgm('SQL036P');
  res char(30);
end-pr;
dcl-s r char(30);
// CLOSQLCSR(*ENDACTGRP) in a *NEW group: closed when the group ends.
child(r);
line = r;
write QSYSPRT line;
child(r);
line = r;
write QSYSPRT line;
*inlr = *on;
