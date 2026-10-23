**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr nothere extpgm('NOTHEREXYZ');
end-pr;
monitor;
  nothere();
on-error 211;
  line = 'not found 211';
  write QSYSPRT line;
on-error;
  line = 'other ' + %char(%status);
  write QSYSPRT line;
endmon;
*inlr = *on;
