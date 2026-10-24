**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds rec;
  a char(2) inz('aa');
end-ds rec;
dcl-pr twice int(10);
  n int(10) value;
end-pr twice;
line = a + ' ' + %char(twice(8));
write QSYSPRT line;
*inlr = *on;
dcl-proc twice;
  dcl-pi twice int(10);
    n int(10) value;
  end-pi twice;
  return n * 2;
end-proc twice;
