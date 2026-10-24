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
// INFSR with a blank return point.
line = 'before';
write QSYSPRT line;
update CUSTR;
line = 'after';
write QSYSPRT line;
*inlr = *on;
begsr errsr;
  line = 'errsr ' + %char(fbstat) + ' ' + fbop + ' ' + %char(%status) + ' ' + %error;
  write QSYSPRT line;
endsr;
