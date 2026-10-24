**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; post: CHGPF FILE({lib}/CUSTF) WAITRCD(1)
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed usage(*update);
dcl-f CUST2 keyed usage(*update) extdesc('CUSTF') extfile(*extdesc)
      rename(CUSTR:C2R) prefix(X_) infsr(lockerr) infds(fb2);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb2;
  fbst *status;
  fbop *opcode;
  fbmsg char(7) pos(46);
end-ds;
dcl-s n int(10);
// A record lock reaches the file's INFSR.
n += 1;
if n > 1;
  line = 'cycle ' + %char(n);
  write QSYSPRT line;
  *inlr = *on;
  return;
endif;
chain (104) CUSTF;
chain (104) CUST2;
line = 'not reached';
write QSYSPRT line;
*inlr = *on;
begsr lockerr;
  line = 'infsr ' + %char(fbst) + ' ' + fbop + ' ' + fbmsg + ' '
       + %char(%status) + ' ' + %char(%error);
  write QSYSPRT line;
endsr '*GETIN';
