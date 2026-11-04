**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f CUSTT extdesc('CUSTF') keyed template block(*no);
dcl-f CUST likefile(CUSTT) extfile('CUSTF') usropn;
dcl-ds r likerec(CUST.CUSTR);
line = 'open ' + %open(CUST);
write QSYSPRT line;
first(CUST);
line = 'after ' + %open(CUST);
write QSYSPRT line;
read CUST r;
line = 'caller ' + %char(r.CUSNO) + ' ' + %eof(CUST);
write QSYSPRT line;
close CUST;
*inlr = *on;

dcl-proc first;
  dcl-pi *n;
    f likefile(CUSTT);
  end-pi;
  dcl-ds x likerec(f.CUSTR);
  if not %open(f);
    open f;
  endif;
  read f x;
  line = 'first ' + %char(x.CUSNO) + ' ' + x.NAME;
  write QSYSPRT line;
  second(f);
end-proc;

dcl-proc second;
  dcl-pi *n;
    g likefile(CUSTT);
  end-pi;
  dcl-ds y likerec(g.CUSTR);
  setgt (105) g;
  read g y;
  line = 'second ' + %char(y.CUSNO) + ' ' + y.CITY;
  write QSYSPRT line;
end-proc;
