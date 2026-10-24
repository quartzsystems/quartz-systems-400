**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; with: SQL034P
//; setup: CRTSQLRPGI OBJ({lib}/SQL034P) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*NONE)
//; cleanup: DLTPGM PGM({lib}/SQL034P)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no) actgrp(*new);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr child extpgm('SQL034P');
  res char(30);
end-pr;
dcl-s r char(30);
// CLOSQLCSR(*ENDACTGRP): the cursor stays open after the program ends.
child(r);
line = r;
write QSYSPRT line;
child(r);
line = r;
write QSYSPRT line;
*inlr = *on;
