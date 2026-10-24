**FREE
//; dds: B8A
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f B8A workstn usropn maxdev(*file);
rel(e) 'COVNODEV' B8A;
line = 'rel ' + %char(%status(B8A)) + ' ' + %error;
write QSYSPRT line;
monitor;
  rel 'COVNODEV' B8A;
  line = 'rel mon ok';
  write QSYSPRT line;
on-error;
  line = 'rel mon ' + %char(%status);
  write QSYSPRT line;
endmon;
*inlr = *on;
