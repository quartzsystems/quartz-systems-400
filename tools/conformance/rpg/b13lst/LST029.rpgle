**FREE
//; run: no
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DFTACTGRP(*NO) ACTGRP(LSTAG) DBGVIEW(*SOURCE) OUTPUT(*PRINT) TEXT('Listing options') TRUNCNBR(*NO)
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = 'opts';
write QSYSPRT line;
*inlr = *on;
