**FREE
//; with: PRC004C
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/copy QRPGLESRC,PRC004C
line = %char(square(12));
write QSYSPRT line;
*inlr = *on;
dcl-proc square;
  dcl-pi *n int(10);
    n int(10) value;
  end-pi;
  return n * n;
end-proc;
