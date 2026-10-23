**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// UPDATE every field, then only %FIELDS.
chain (102) CUSTF;
BAL = BAL + 100;
NAME = 'Robert Brown';
update CUSTR;
chain (103) CUSTF;
BAL = 33.33;
CITY = 'Nowhere';
update CUSTR %fields(BAL);
chain (105) CUSTR;
NAME = 'Evelyn Evans';
update CUSTR %fields(NAME:CITY);
read CUSTF;
setll *loval CUSTF;
read(n) CUSTF;
dow not %eof(CUSTF);
line = %char(CUSNO) + ' ' + NAME + CITY + %char(BAL);
write QSYSPRT line;
read(n) CUSTF;
enddo;
*inlr = *on;
