**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update:*output) infsr(errsr) infds(fb);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb;
  fbstat *status;
end-ds;
dcl-s z packed(3:0) inz(0);
dcl-s r packed(5:0);
dcl-s n int(10);
// Errors inside a subroutine: INFSR and *PSSR still see their status.
n += 1;
if n = 1;
  exsr dowrite;
endif;
line = 'detail ' + %char(n);
write QSYSPRT line;
exsr divide;
*inlr = *on;
begsr dowrite;
  CUSNO = 108;
  write CUSTR;
  line = 'not here';
  write QSYSPRT line;
endsr;
begsr divide;
  r = 1 / z;
endsr;
begsr errsr;
  line = 'errsr ' + %char(%status) + ' ' + %char(fbstat);
  write QSYSPRT line;
endsr '*DETC';
begsr *pssr;
  line = 'pssr ' + %char(%status);
  write QSYSPRT line;
  *inlr = *on;
  return;
endsr;
