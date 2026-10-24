**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f custf keyed qualified;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds rec likerec(custf.custr);
read custf.custr rec;
dow not %eof(custf);
  line = %char(rec.cusno) + ' ' + %trim(rec.city) + ' ' + %char(%size(rec));
  write QSYSPRT line;
  read custf.custr rec;
enddo;
*inlr = *on;
