**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usropn infsr(*pssr) infds(fb);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb;
  fstat *status;
  fop *opcode;
end-ds;
dcl-s key packed(5:0) inz(101);
dcl-s n int(10);
// INFSR(*PSSR) on a closed file, *DETC, then a program error.
n += 1;
if n = 1;
  chain key CUSTF;
endif;
line = 'detail ' + %char(n);
write QSYSPRT line;
key = key / (n - 2);
*inlr = *on;
if n = 99;
  open CUSTF;
endif;
begsr *pssr;
  line = 'pssr ' + %char(n) + ' ' + %char(%status);
  write QSYSPRT line;
  if %status = 1211;
    line = 'infds ' + %char(fstat) + ' ' + fop;
    write QSYSPRT line;
  endif;
  if %status = 102;
    *inlr = *on;
    return;
  endif;
endsr '*DETC';
