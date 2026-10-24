**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s code char(4) inz('glob');
dcl-ds one qualified;
  code char(4) inz('one');
end-ds;
dcl-ds two qualified;
  code char(4) inz('two');
  one char(2) inz('11');
end-ds;
line = code + ' ' + one.code + ' ' + two.code + ' ' + two.one;
write QSYSPRT line;
*inlr = *on;
