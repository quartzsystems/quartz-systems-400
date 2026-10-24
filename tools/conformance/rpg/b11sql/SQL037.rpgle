**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; with: SQL037P
//; setup: CRTSQLRPGI OBJ({lib}/SQL037P) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE)
//; cleanup: DLTPGM PGM({lib}/SQL037P)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no) actgrp(*new);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr child extpgm('SQL037P');
  res char(30);
end-pr;
dcl-s r char(30);
// SET OPTION CLOSQLCSR = *ENDMOD, a return without LR: the cursor
// closes and the prepared statement goes.
r = 'first';
child(r);
line = r;
write QSYSPRT line;
r = 'again';
child(r);
line = r;
write QSYSPRT line;
*inlr = *on;
