**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
callp opt(1);
*inlr = *on;
dcl-proc opt;
  dcl-pi *n;
    a int(10) const;
  end-pi;
  line = 'o ' + %omitted(a);
  write QSYSPRT line;
end-proc;
