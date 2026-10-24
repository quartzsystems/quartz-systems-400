**FREE
//; with: PRC010P
//; setup: CRTBNDRPG PGM({lib}/PRC010P) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC010P) DBGVIEW(*NONE)
//; cleanup: DLTPGM PGM({lib}/PRC010P)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr callee extpgm('PRC010P');
  t packed(7:2);
  n char(10) options(*nopass);
end-pr;
dcl-s amt packed(7:2) inz(3.25);
dcl-s txt char(10) inz('one');
callee(amt : txt);
line = %char(amt) + ' ' + txt;
write QSYSPRT line;
callee(amt);
line = %char(amt) + ' ' + txt;
write QSYSPRT line;
*inlr = *on;
