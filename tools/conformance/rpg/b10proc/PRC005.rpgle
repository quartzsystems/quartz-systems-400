**FREE
//; with: PRC005D
//; setup: CRTRPGMOD MODULE({lib}/PRC005D) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC005D)
//; setup: CRTRPGMOD MODULE({lib}/PRC005) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC005)
//; cmd: CRTPGM PGM({lib}/PRC005) MODULE({lib}/PRC005 {lib}/PRC005D)
//; cleanup: DLTMOD MODULE({lib}/PRC005D)
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s counter int(10) import;
dcl-pr tick end-pr;
line = 'start ' + %char(counter);
write QSYSPRT line;
tick();
tick();
line = 'after ' + %char(counter);
write QSYSPRT line;
*inlr = *on;
