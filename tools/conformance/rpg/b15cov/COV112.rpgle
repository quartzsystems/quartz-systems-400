**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds c extname('CUSTF') qualified;
  who extfld('NAME');
  bal extfld extfmt('S');
end-ds;
c.who = 'renamed';
c.bal = -1.25;
line = c.who + ' ' + %editc(c.bal:'X') + ' ' + %char(%size(c.bal)) + ' ' + %char(%size(c));
write QSYSPRT line;
*inlr = *on;
