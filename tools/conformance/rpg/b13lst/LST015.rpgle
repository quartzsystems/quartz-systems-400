**FREE
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT) OPTION(*NOEXPDDS)
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds keys extname('CUSTF':*KEY) qualified end-ds;
keys.CUSNO = 101;
chain %kds(keys) CUSTF;
if %found(CUSTF);
  BAL += 1;
  update CUSTR;
endif;
line = NAME + %char(BAL);
write QSYSPRT line;
*inlr = *on;
