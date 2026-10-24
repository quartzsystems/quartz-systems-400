**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f CUSTF disk(40) keyed(*char:3) keyloc(1);
dcl-ds rec len(40);
  name char(20) pos(4);
end-ds;
dcl-ds key;
  k packed(5:0);
end-ds;
monitor;
  k = 104;
  chain key CUSTF rec;
  line = 'chain ' + %found(CUSTF) + ' ' + name;
  write QSYSPRT line;
  line = 'keyloc ok';
  write QSYSPRT line;
on-error;
  line = 'keyloc ' + %char(%status);
  write QSYSPRT line;
endmon;
monitor;
  k = 999;
  chain key CUSTF rec;
  line = 'chain ' + %found(CUSTF);
  write QSYSPRT line;
  line = 'keyloc 999 ok';
  write QSYSPRT line;
on-error;
  line = 'keyloc 999 ' + %char(%status);
  write QSYSPRT line;
endmon;
*inlr = *on;
