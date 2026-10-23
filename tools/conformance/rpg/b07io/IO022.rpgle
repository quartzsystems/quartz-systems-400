**FREE
//; dds: HISTF
//; sqlfile: HISTF
ctl-opt dftactgrp(*no);
dcl-f HISTF infds(fb);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb;
  rrn int(10) pos(397);
end-ds;
// Arrival sequence: CHAIN and SETLL by relative record number.
chain 3 HISTF;
line = %char(HCUS) + ' ' + HTEXT + %char(HAMT) + ' rrn ' + %char(rrn);
write QSYSPRT line;
chain 9 HISTF;
line = 'chain 9 ' + %found(HISTF);
write QSYSPRT line;
setll 4 HISTF;
read HISTF;
line = %char(HCUS) + ' ' + HTEXT + ' rrn ' + %char(rrn);
write QSYSPRT line;
setll *start HISTF;
read HISTF;
dow not %eof(HISTF);
line = %char(rrn) + ': ' + %char(HCUS) + ' ' + HTEXT + %char(HAMT);
write QSYSPRT line;
read HISTF;
enddo;
*inlr = *on;
