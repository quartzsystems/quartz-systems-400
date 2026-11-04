**FREE
//; cmd: CRTRPGMOD MODULE({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name})
//; cmd+: DBGVIEW(*NONE) OUTPUT(*PRINT)
//; run: no
//; dds: CUSTF
ctl-opt nomain;
dcl-f CUSTF keyed usage(*input) block(*no) template;
dcl-proc total export;
  dcl-pi *n packed(11:2);
    f likefile(CUSTF);
    city char(12) const;
  end-pi;
  dcl-ds x likerec(f.CUSTR:*ALL);
  dcl-s t packed(11:2);
  setll *start f;
  read f x;
  dow not %eof(f);
    if x.CITY = city;
      t += x.BAL;
    endif;
    read f x;
  enddo;
  return t;
end-proc;
