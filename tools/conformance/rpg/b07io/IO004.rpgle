**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
// Positioning: *START, *END, *LOVAL, *HIVAL, SETGT; READP.
setll *end CUSTF;
readp CUSTF;
line = 'last ' + %char(CUSNO) + ' ' + %eof(CUSTF);
write QSYSPRT line;
setll *start CUSTF;
read CUSTF;
line = 'first ' + %char(CUSNO);
write QSYSPRT line;
setgt *hival CUSTF;
readp CUSTF;
line = 'hival ' + %char(CUSNO);
write QSYSPRT line;
setll *loval CUSTF;
read CUSTF;
line = 'loval ' + %char(CUSNO);
write QSYSPRT line;
setgt (104) CUSTF;
read CUSTF;
line = 'gt 104 read ' + %char(CUSNO);
write QSYSPRT line;
setgt (104) CUSTF;
readp CUSTF;
line = 'gt 104 readp ' + %char(CUSNO);
write QSYSPRT line;
setll (103) CUSTF;
readp CUSTF;
dow not %eof(CUSTF);
line = 'back ' + %char(CUSNO);
write QSYSPRT line;
readp CUSTF;
enddo;
line = 'bof ' + %eof(CUSTF);
write QSYSPRT line;
*inlr = *on;
