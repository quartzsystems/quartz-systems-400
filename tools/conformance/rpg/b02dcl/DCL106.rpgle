**FREE
//; dds: DFTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds d1 extname('DFTF') inz(*extdft) end-ds;
dcl-ds d2 extname('DFTF') qualified inz end-ds;
dcl-ds d3 extname('DFTF') qualified inz(*extdft);
  dqty extfld inz(9);
end-ds;
line = dcode + ' ' + %char(dqty) + ' ' + %char(damt) + ' [' + dname + '] ' + %char(dcnt);
write QSYSPRT line;
line = %char(ddate) + ' [' + dtext + '] ' + %char(%len(dtext));
write QSYSPRT line;
line = '[' + d2.dcode + '] ' + %char(d2.dqty) + ' ' + %char(d2.ddate) + ' ' +
  %char(%len(d2.dtext));
write QSYSPRT line;
line = d3.dcode + ' ' + %char(d3.dqty);
write QSYSPRT line;
*inlr = *on;
