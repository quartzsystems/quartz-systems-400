**FREE
ctl-opt dftactgrp(*no) pgminfo(*pcml:*module);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = 'v ' + get();
write QSYSPRT line;
*inlr = *on;
dcl-proc get pgminfo(*no) reqproto(*no);
  dcl-pi *n varchar(20) rtnparm;
  end-pi;
  return 'rtnparm ' + %char(%parms);
end-proc;
