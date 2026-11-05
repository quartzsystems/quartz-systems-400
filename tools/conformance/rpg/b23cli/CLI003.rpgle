**FREE
//; call: CALL PGM({lib}/{name}) PARM('{lib}')
//; with: CLIH
//; sqlfile: CLIT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/copy CLIH
// The catalog functions: each result's columns, then its rows from the
// third column on (the first two name the system and the library).
dcl-pi *n;
  lib char(10);
end-pi;
dcl-s henv int(10);
dcl-s hdbc int(10);
dcl-s hstmt int(10);
dcl-s rc int(10);
dcl-s dsn char(10) inz('*LOCAL');
dcl-s tab char(10) inz('CLIT');
dcl-s pat char(10) inz('CLI%');
dcl-s col char(10) inz('A%');
dcl-s typ char(10) inz('TABLE');
dcl-s c128 char(128);
dcl-s nm char(40);
dcl-s l1 int(10);
dcl-s t int(5);
dcl-s sc int(5);
dcl-s nl int(5);
dcl-s si int(5);
dcl-s def int(10);

rc = SQLAllocEnv(%addr(henv));
rc = SQLAllocConnect(henv : %addr(hdbc));
rc = SQLConnect(hdbc : %addr(dsn) : 6 : *null : 0 : *null : 0);
rc = SQLAllocStmt(hdbc : %addr(hstmt));
rc = SQLTables(hstmt : *null : 0 : %addr(lib) : %len(%trimr(lib)) : %addr(pat) : 4
               : %addr(typ) : 5);
say('tables ' + %char(rc));
cols(1);
rc = SQLCloseCursor(hstmt);
rc = SQLColumns(hstmt : *null : 0 : %addr(lib) : %len(%trimr(lib)) : %addr(tab) : 4
                : *null : 0);
say('columns ' + %char(rc));
cols(1);
rc = SQLCloseCursor(hstmt);
rc = SQLColumns(hstmt : *null : 0 : %addr(lib) : %len(%trimr(lib)) : %addr(tab) : 4
                : %addr(col) : 2);
say('columns-a ' + %char(rc));
cols(0);
rc = SQLCloseCursor(hstmt);
rc = SQLPrimaryKeys(hstmt : *null : 0 : %addr(lib) : %len(%trimr(lib)) : %addr(tab) : 4);
say('primarykeys ' + %char(rc));
cols(1);
rc = SQLCloseCursor(hstmt);
rc = SQLPrimaryKeys(hstmt : *null : 0 : %addr(lib) : %len(%trimr(lib)) : *null : 0);
say('primarykeys-none ' + %char(rc));
rc = SQLFreeStmt(hstmt : SQL_DROP);
rc = SQLDisconnect(hdbc);
rc = SQLFreeConnect(hdbc);
rc = SQLFreeEnv(henv);
*inlr = *on;

dcl-proc cols;
  dcl-pi *n;
    described int(10) value;
  end-pi;
  dcl-s r int(10);
  dcl-s c int(5);
  dcl-s nc int(5);
  dcl-s rows int(10);
  dcl-s txt varchar(300);
  r = SQLNumResultCols(hstmt : %addr(nc));
  say('  numcols ' + %char(r) + ' ' + %char(nc));
  for c = 1 to nc;
    if described = 0;
      leave;
    endif;
    nm = *all'*';
    r = SQLDescribeCol(hstmt : c : %addr(nm) : 40 : %addr(nl) : %addr(t) : %addr(def)
                       : %addr(sc) : %addr(si));
    say('  col ' + %char(c) + ' ' + %str(%addr(nm) : 40) + ' ' + %char(t) + ' ' + %char(def) + ' '
        + %char(sc) + ' ' + %char(si));
  endfor;
  rows = 0;
  dow rows < 20;
    r = SQLFetch(hstmt);
    if r <> 0;
      leave;
    endif;
    rows += 1;
    txt = '  row';
    for c = 3 to nc;
      // The key's name holds the library's.
      if c = 6 and nc = 6;
        leave;
      endif;
      c128 = *blanks;
      r = SQLGetData(hstmt : c : SQL_CHAR : %addr(c128) : 128 : %addr(l1));
      if l1 = SQL_NULL_DATA;
        txt += ' |-';
      else;
        txt += ' |' + %trimr(%scanrpl(x'00' : '' : c128));
      endif;
    endfor;
    say(%subst(txt : 1 : %min(%len(txt) : 132)));
  enddo;
  say('  fetch ' + %char(r) + ' ' + %char(rows));
end-proc;

dcl-proc say;
  dcl-pi *n;
    text varchar(132) const;
  end-pi;
  line = text;
  write QSYSPRT line;
end-proc;
