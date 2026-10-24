**FREE
//; with: PRC011P
//; setup: CRTBNDRPG PGM({lib}/PRC011P) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC011P) DBGVIEW(*NONE)
//; cleanup: DLTPGM PGM({lib}/PRC011P)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr callee extpgm('PRC011P');
  a int(10);
  b char(3) options(*nopass);
end-pr;
dcl-s n int(10) inz(41);
dcl-s c char(3) inz('no');
callee(n);
line = %char(n) + ' ' + c;
write QSYSPRT line;
callee(n : c);
line = %char(n) + ' ' + c;
write QSYSPRT line;
*inlr = *on;
