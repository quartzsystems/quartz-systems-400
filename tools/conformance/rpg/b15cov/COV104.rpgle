**FREE
//; dds: COVALI
//; sql: INSERT INTO {lib}.COVALI VALUES (1, 'first'), (2, 'second')
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f COVALI disk usage(*input) alias qualified;
dcl-ds r likerec(COVALI.ALIR);
read COVALI r;
dow not %eof(COVALI);
  line = %char(r.item_number) + ' ' + r.item_description;
  write QSYSPRT line;
  read COVALI r;
enddo;
*inlr = *on;
