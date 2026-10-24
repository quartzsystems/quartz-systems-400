**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s v varchar(20);
dcl-ds pt qualified;
  xval int(10);
  yval int(10);
end-ds;
xml-into v %xml('<v>plain text</v>':'doc=string');
xml-into pt %xml('<Pt><XVal>3</XVal><yVAL>-4</yVAL></Pt>':'doc=string case=any');
line = '[' + v + '] ' + %char(pt.xval) + ' ' + %char(pt.yval);
write QSYSPRT line;
*inlr = *on;
