**FREE
//; dds: TYPESF
//; sqlfile: TYPESF
ctl-opt dftactgrp(*no);
dcl-f TYPESF keyed usage(*update:*output);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// Writing and updating fields of each type.
TKEY = 9;
TZON = -1.5;
TBIN = 77;
TBIN2 = 3.25;
TDATE = d'2024-02-29';
TTIME = t'08.30.00';
TSTMP = z'2024-02-29-08.30.00.000500';
TVAR = 'written';
write TYPR;
chain (2) TYPESF;
TDATE = d'2001-01-01';
TVAR = %trim(TVAR) + '!';
update TYPR;
setll *start TYPESF;
read(n) TYPESF;
dow not %eof(TYPESF);
line = %char(TKEY) + ' ' + %char(TZON) + ' ' + %char(TBIN) + ' ' + %char(TBIN2);
write QSYSPRT line;
line = '  ' + %char(TDATE) + ' ' + %char(TTIME) + ' ' + %char(TSTMP) + ' [' + TVAR + ']';
write QSYSPRT line;
read(n) TYPESF;
enddo;
*inlr = *on;
