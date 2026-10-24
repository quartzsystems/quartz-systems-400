**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
DCL-S MyVar Int(10) Inz(4);
myvar = MYVAR * 2;
If MyVaR = 8;
  Line = %Char(myVAR) + ' ' + %TRIM('  x ') + %trim(' y');
  Write qsysprt LINE;
EndIf;
*INLR = *on;
