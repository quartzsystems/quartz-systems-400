**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
//; with: SQL038P
//; setup: CRTSQLRPGI OBJ({lib}/SQL038P) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE)
//; cleanup: DLTPGM PGM({lib}/SQL038P)
//; sqlfile: EMPT
ctl-opt dftactgrp(*no) actgrp(*new);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr child extpgm('SQL038P');
  res char(30);
end-pr;
dcl-s r char(30);
// *ENDACTGRP, a return without LR: the cursor and the prepared
// statement stay.
r = 'first';
child(r);
line = r;
write QSYSPRT line;
r = 'again';
child(r);
line = r;
write QSYSPRT line;
*inlr = *on;
