**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s
  x
  packed(
    7
    :
    2
  )
  inz(
   1.5
  );
if
  x
  >
  1
;
  line = %char(x);
  write QSYSPRT line;
endif;
*inlr = *on;
