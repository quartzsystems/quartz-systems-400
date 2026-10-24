**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds info qualified;
  name varchar(10);
end-ds;
monitor;
  data-into info %data('name=x':'doc=string') %parser('COVNOPRS');
  line = 'into ok';
  write QSYSPRT line;
on-error;
  line = 'into ' + %char(%status);
  write QSYSPRT line;
endmon;
*inlr = *on;
