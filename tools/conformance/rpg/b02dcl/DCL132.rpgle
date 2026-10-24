**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds pgmsts psds;
  proc *proc;
  stat *status;
  rtn *routine;
  parms *parms;
end-ds;
dcl-s z int(10) inz(0);
dcl-s r int(10);
line = %trim(proc) + ' ' + %char(stat) + ' ' + %trim(rtn) + ' ' + %char(parms) + ' ' +
  %char(%size(pgmsts));
write QSYSPRT line;
monitor;
  r = 10 / z;
on-error;
line = 'after error ' + %char(stat) + ' ' + %char(%status);
  write QSYSPRT line;
endmon;
*inlr = *on;
