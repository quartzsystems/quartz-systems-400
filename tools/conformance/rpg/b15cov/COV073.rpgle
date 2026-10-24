**FREE
ctl-opt dftactgrp(*no) aut(*use) usrprf(*user) stgmdl(*snglvl) langid(*jobrun) text('cover test');
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
line = 'more object keywords';
write QSYSPRT line;
*inlr = *on;
