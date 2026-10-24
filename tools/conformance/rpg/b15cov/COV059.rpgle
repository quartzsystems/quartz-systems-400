**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s tabx char(1) dim(3) ctdata perrcd(3);
if %tlookuplt('b':tabx);
  line = tabx;
  write QSYSPRT line;
endif;
*inlr = *on;
**ctdata tabx
ace
