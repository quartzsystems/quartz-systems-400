**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s tabkey char(1) dim(5) ctdata perrcd(5) ascend;
dcl-s tabval char(5) dim(5) alt(tabkey);
if %tlookuplt('c':tabkey:tabval);
  line = 'lt ' + tabkey + ' ' + tabval;
  write QSYSPRT line;
endif;
if %tlookupge('c':tabkey:tabval);
  line = 'ge ' + tabkey + ' ' + tabval;
  write QSYSPRT line;
endif;
if not %tlookuplt('a':tabkey);
  line = 'none below a';
  write QSYSPRT line;
endif;
if %tlookupge('z':tabkey);
  line = 'z found';
  write QSYSPRT line;
else;
  line = 'nothing at or above z, still ' + tabkey;
  write QSYSPRT line;
endif;
*inlr = *on;
**ctdata tabkey
aone  btwo  dfour fsix  height
