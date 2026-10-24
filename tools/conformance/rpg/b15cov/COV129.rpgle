**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f CUSTF keyed usage(*input) block(*no);
callp show(CUSTF);
*inlr = *on;
dcl-proc show;
  dcl-pi *n;
    f likefile(CUSTF);
  end-pi;
  dcl-ds r2 likerec(CUSTR);
  chain (105) f r2;
  line = 'param ' + r2.name;
  write QSYSPRT line;
end-proc;
