**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n packed(5:0) noopt inz(5);
dcl-pr desc varchar(20) opdesc rtnparm;
  c char(10) const options(*varsize);
end-pr;
n += 1;
line = 'n ' + %char(n) + ' ' + desc('abc');
write QSYSPRT line;
*inlr = *on;
dcl-proc desc;
  dcl-pi *n varchar(20) opdesc rtnparm;
    c char(10) const options(*varsize);
  end-pi;
  return 'parms ' + %char(%parms);
end-proc;
