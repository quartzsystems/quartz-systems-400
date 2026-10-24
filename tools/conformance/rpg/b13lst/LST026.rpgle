**FREE
//; run: no
//; cmd: CRTRPGMOD MODULE({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT)
ctl-opt nomain;
dcl-s shared int(10) export inz(5);
dcl-s outside int(10) import;
dcl-pr helper int(10) extproc('LSTHELP');
  n int(10) value;
end-pr;
dcl-proc bump export;
  dcl-pi *n int(10);
  end-pi;
  return helper(shared + outside);
end-proc;
