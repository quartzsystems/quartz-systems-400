**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s tc char(5) template inz('tchr');
dcl-s c2 like(tc);
dcl-ds td template;
  f1 packed(5:0) inz(5);
end-ds;
dcl-ds d1 likeds(td) inz(*likeds);
line = '[' + c2 + '] ' + %char(%size(tc)) + ' ' + %char(d1.f1) + ' ' + %char(%size(td));
write QSYSPRT line;
*inlr = *on;
