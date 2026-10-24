**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
callp opt(1);
callp opt(1:2);
callp opt(1:*omit);
*inlr = *on;
dcl-proc opt;
  dcl-pi *n;
    a int(10) const;
    b int(10) const options(*nopass:*omit);
  end-pi;
  line = 'parms ' + %char(%parms) + ' passed ' + %passed(b) + ' omitted ' + %omitted(b);
  write QSYSPRT line;
end-proc;
