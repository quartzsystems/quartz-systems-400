**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
monitor;
  snd-msg *escape %msg('CPF9898':'QCPFMSG':'boom');
on-excp 'CPF9898';
  line = 'caught CPF9898 ' + %char(%status);
  write QSYSPRT line;
endmon;
*inlr = *on;
