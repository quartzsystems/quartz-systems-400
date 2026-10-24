**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = 'main ' + %proc();
write QSYSPRT line;
callp sub1();
callp Sub_Two();
*inlr = *on;
dcl-proc sub1;
  line = 'sub ' + %proc();
  write QSYSPRT line;
end-proc;
dcl-proc Sub_Two;
  dcl-pi *n extproc(*dclcase);
  end-pi;
  line = 'sub ' + %proc() + ' ' + %char(%len(%proc()));
  write QSYSPRT line;
end-proc;
