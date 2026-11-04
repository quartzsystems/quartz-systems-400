**FREE
//; with: OA003H
//; setup: CRTBNDRPG PGM({lib}/OA003H) SRCFILE({lib}/QRPGLESRC) SRCMBR(OA003H) DBGVIEW(*NONE)
//; cleanup: DLTPGM PGM({lib}/OA003H)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds log qualified inz;
  n int(10);
  l char(100) dim(40);
end-ds;
dcl-f RPT printer(40) handler('OA003H' : log) oflind(ovf);
dcl-ds prt len(40) end-ds;
dcl-s ovf ind;
dcl-s i int(10);
prt = 'first line';
write RPT prt;
prt = 'second line';
write RPT prt;
line = 'ovf ' + ovf;
write QSYSPRT line;
prt = 'third line';
write RPT prt;
line = 'ovf ' + ovf;
write QSYSPRT line;
close RPT;
for i = 1 to log.n;
  line = log.l(i);
  write QSYSPRT line;
endfor;
*inlr = *on;
