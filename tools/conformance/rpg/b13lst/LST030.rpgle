**FREE
//; run: no
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE)
//; cmd+: OUTPUT(*PRINT) DEFINE(ONE TWO) OPTIMIZE(*BASIC) ALWNULL(*USRCTL)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/if defined(TWO)
dcl-s two char(3) inz('two');
/endif
line = two;
write QSYSPRT line;
*inlr = *on;
