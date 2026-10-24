**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds pgmsts psds qualified;
  status *status;
  excid char(7) pos(40);
end-ds;
// A file error in a subprocedure reaches the caller's MONITOR.
monitor;
  dup(106);
  line = 'not here';
  write QSYSPRT line;
on-error 1021;
  line = 'caller 1021';
  write QSYSPRT line;
on-error;
  line = 'caller ' + %char(%status) + ' ' + pgmsts.excid;
  write QSYSPRT line;
endmon;
dup(107);
line = 'after';
write QSYSPRT line;
*inlr = *on;
dcl-proc dup;
  dcl-pi *n;
    k packed(5:0) value;
  end-pi;
  CUSNO = k;
  write CUSTR;
  line = 'proc after write';
  write QSYSPRT line;
end-proc;
