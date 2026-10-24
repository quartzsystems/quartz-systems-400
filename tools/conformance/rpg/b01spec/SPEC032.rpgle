**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f custf            // the customer file
      usage(*input)    // read only
      keyed
      rename(custr : cr)
      prefix(k_);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
chain (104) cr;
if %found(custf);
  line = %char(k_cusno) + ' ' + %trim(k_name);
  write QSYSPRT line;
endif;
chain (999) cr;
if not %found(custf);
  line = '999 not found';
  write QSYSPRT line;
endif;
*inlr = *on;
