**FREE
//; with: PRC003S
//; setup: CRTRPGMOD MODULE({lib}/PRC003S) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC003S)
//; setup: CRTSRVPGM SRVPGM({lib}/PRC003S) MODULE({lib}/PRC003S) EXPORT(*ALL)
//; setup: CRTRPGMOD MODULE({lib}/PRC003) SRCFILE({lib}/QRPGLESRC) SRCMBR(PRC003)
//; cmd: CRTPGM PGM({lib}/PRC003) MODULE({lib}/PRC003) BNDSRVPGM({lib}/PRC003S)
//; cleanup: DLTSRVPGM SRVPGM({lib}/PRC003S)
//; cleanup: DLTMOD MODULE({lib}/PRC003S)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-pr greet varchar(30);
  who varchar(10) const;
end-pr;
dcl-pr addone int(10);
  n int(10) value;
end-pr;
line = greet('world');
write QSYSPRT line;
line = %char(addone(41));
write QSYSPRT line;
*inlr = *on;
