**FREE
//; cmd: CRTRPGMOD MODULE({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name})
//; cmd+: DBGVIEW(*NONE) OUTPUT(*PRINT)
//; run: no
//; dds: CUSTF
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f CUSTF keyed usage(*input) block(*no);
dcl-ds r likerec(CUSTR);
dcl-pr total packed(11:2);
  f likefile(CUSTF);
  city char(12) const;
end-pr;
line = 'boston ' + %char(total(CUSTF : 'Boston'));
write QSYSPRT line;
line = 'eof ' + %eof(CUSTF);
write QSYSPRT line;
chain (106) CUSTF r;
line = 'chain ' + r.NAME;
write QSYSPRT line;
*inlr = *on;
