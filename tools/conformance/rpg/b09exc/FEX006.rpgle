**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output) infsr(errsr) infds(fb);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb;
  fbstat *status;
  fbop *opcode;
end-ds;
dcl-s n int(10);
// INFSR returning to *DETC: the detail calculations again.
n += 1;
line = 'detail ' + %char(n);
write QSYSPRT line;
if n >= 3;
  *inlr = *on;
  return;
endif;
CUSNO = 103;
write CUSTR;
line = 'after write';
write QSYSPRT line;
*inlr = *on;
begsr errsr;
  line = 'errsr ' + %char(fbstat) + ' ' + fbop;
  write QSYSPRT line;
endsr '*DETC';
