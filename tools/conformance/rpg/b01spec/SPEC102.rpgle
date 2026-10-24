**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/define A
/define B
/if defined(A)
/if defined(B)
line = 'a and b';
write QSYSPRT line;
/else
line = 'a only';
write QSYSPRT line;
/endif
/undefine B
/if defined(B)
line = 'b still';
write QSYSPRT line;
/elseif defined(A)
line = 'b gone';
write QSYSPRT line;
/else
line = 'neither';
write QSYSPRT line;
/endif
/endif
*inlr = *on;
