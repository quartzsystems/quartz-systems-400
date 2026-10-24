**FREE
//; with: COV099P
//; setup: CRTBNDRPG PGM({lib}/COV099P) SRCFILE({lib}/QRPGLESRC) SRCMBR(COV099P) OUTPUT(*NONE)
//; cleanup: DLTPGM PGM({lib}/COV099P)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f SPCL special(20) usage(*input) pgmname('COV099P');
dcl-ds rec len(20) end-ds;
read SPCL rec;
dow not %eof(SPCL);
  line = 'got ' + rec;
  write QSYSPRT line;
  read SPCL rec;
enddo;
line = 'eof';
write QSYSPRT line;
*inlr = *on;
