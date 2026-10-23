**FREE
//; dds: TYPESF
//; sqlfile: TYPESF
ctl-opt dftactgrp(*no);
dcl-f TYPESF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// Fields of each type from an external description.
read TYPESF;
dow not %eof(TYPESF);
line = %char(TKEY) + ' ' + %char(TZON) + ' ' + %char(TBIN) + ' ' + %char(TBIN2);
write QSYSPRT line;
line = %char(TDATE) + ' ' + %char(TTIME) + ' ' + %char(TSTMP);
write QSYSPRT line;
line = '[' + TVAR + '] ' + %char(%len(TVAR));
write QSYSPRT line;
read TYPESF;
enddo;
line = %char(%size(TZON)) + ' ' + %char(%size(TBIN)) + ' ' + %char(%size(TBIN2));
write QSYSPRT line;
*inlr = *on;
