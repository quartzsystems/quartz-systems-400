**FREE
//; run: no
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE)
//; cmd+: OUTPUT(*PRINT) OPTION(*NOXREF)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s used int(10) inz(1);
dcl-s unused char(3);
dcl-s spare packed(5:2);
dcl-pr ext1 extproc('LSTEXT1');
end-pr;
line = %char(used);
write QSYSPRT line;
*inlr = *on;
