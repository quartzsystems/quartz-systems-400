**FREE
//; with: COV094S
//; setup: CRTRPGMOD MODULE({lib}/COV094S) SRCFILE({lib}/QRPGLESRC) SRCMBR(COV094S) OUTPUT(*NONE)
//; setup: CRTSRVPGM SRVPGM({lib}/COV094S) MODULE({lib}/COV094S) EXPORT(*ALL)
//; setup: CRTBNDDIR BNDDIR({lib}/COVBD)
//; setup: ADDBNDDIRE BNDDIR({lib}/COVBD) OBJ(({lib}/COV094S *SRVPGM))
//; cleanup: DLTBNDDIR BNDDIR({lib}/COVBD)
//; cleanup: DLTSRVPGM SRVPGM({lib}/COV094S)
//; cleanup: DLTMOD MODULE({lib}/COV094S)
ctl-opt dftactgrp(*no) bnddir('COVBD');
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr dbl int(10);
  n int(10) value;
end-pr;
line = 'bnddir ' + %char(dbl(21));
write QSYSPRT line;
*inlr = *on;
