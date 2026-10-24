**FREE
//; with: PRC006P
//; setup: CRTBNDRPG PGM({lib}/PRC006P) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC006P) DBGVIEW(*NONE)
//; cleanup: DLTPGM PGM({lib}/PRC006P)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr callee extpgm('PRC006P');
  a packed(7:2);
  t char(10);
end-pr;
dcl-s amt packed(7:2) inz(2.5);
dcl-s txt char(10) inz('before');
callee(amt : txt);
line = %char(amt) + ' ' + txt;
write QSYSPRT line;
*inlr = *on;
