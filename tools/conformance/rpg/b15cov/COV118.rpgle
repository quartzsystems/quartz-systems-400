**FREE
//; dds: COVNUL
//; sql: INSERT INTO {lib}.COVNUL VALUES (1, 'one'), (2, NULL), (3, 'three')
ctl-opt dftactgrp(*no) alwnull(*usrctl);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f COVNUL disk usage(*input);
read COVNUL;
dow not %eof(COVNUL);
  line = %char(NKEY) + ' ' + %nullind(NVAL) + ' [' + NVAL + ']';
  write QSYSPRT line;
  read COVNUL;
enddo;
*inlr = *on;
