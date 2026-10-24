**FREE
//; with: LSTC1
//; run: no
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE)
//; cmd+: OUTPUT(*PRINT) OPTION(*NOSHOWCPY)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/copy QRPGLESRC,LSTC1
dcl-s r int(10);
r = twice(21);
line = %char(r);
write QSYSPRT line;
*inlr = *on;
dcl-proc twice;
  dcl-pi *n int(10);
    n int(10) value;
  end-pi;
  return n * 2;
end-proc;
