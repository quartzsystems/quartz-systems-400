**FREE
//; call: CALL PGM({lib}/{name}) PARM('{lib}')
//; with: CLIH
//; sqlfile: CLIT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/copy CLIH
// Buffers by C type, parameters, scrolling and commitment control.
dcl-pi *n;
  lib char(10);
end-pi;
dcl-s henv int(10);
dcl-s hdbc int(10);
dcl-s hstmt int(10);
dcl-s h2 int(10);
dcl-s rc int(10);
dcl-s v int(10);
dcl-s dsn char(10) inz('*LOCAL');
dcl-s stmt char(300);
dcl-s c2 char(2);
dcl-s c10 char(10);
dcl-s l1 int(10);
dcl-s l2 int(10);
dcl-s l3 int(10);
dcl-s l4 int(10);
dcl-s i4 int(10);
dcl-s cnt int(10);
dcl-s pk packed(5:1);
dcl-s zn zoned(7:3);
dcl-s fl float(8);
dcl-s bi int(20);
dcl-s si int(5);
dcl-s vc varchar(20);
dcl-s pid int(10);
dcl-s pname char(20);
dcl-s pnl int(10);
dcl-s pamt packed(9:2);
dcl-s c20 char(20);
dcl-s state char(6);
dcl-s native int(10);
dcl-s msg char(300);
dcl-s mlen int(5);

rc = SQLAllocEnv(%addr(henv));
rc = SQLAllocConnect(henv : %addr(hdbc));
rc = SQLConnect(hdbc : %addr(dsn) : 6 : *null : 0 : *null : 0);
rc = SQLAllocStmt(hdbc : %addr(hstmt));

stmt = 'SELECT NAME, D, AMT, AMT, AMT, ID, ID, NAME FROM ' + %trim(lib)
     + '.CLIT ORDER BY ID';
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
rc = SQLBindCol(hstmt : 1 : SQL_CHAR : %addr(c2) : 2 : %addr(l1));
rc = SQLBindCol(hstmt : 2 : SQL_CHAR : %addr(c10) : 10 : %addr(l2));
rc = SQLBindCol(hstmt : 3 : SQL_NUMERIC : %addr(zn) : 7 * 256 + 3 : %addr(l3));
rc = SQLBindCol(hstmt : 4 : SQL_DECIMAL : %addr(pk) : 5 * 256 + 1 : %addr(l4));
rc = SQLBindCol(hstmt : 5 : SQL_DOUBLE : %addr(fl) : 8 : %addr(i4));
rc = SQLBindCol(hstmt : 6 : SQL_BIGINT : %addr(bi) : 8 : *null);
rc = SQLBindCol(hstmt : 7 : SQL_SMALLINT : %addr(si) : 2 : *null);
rc = SQLBindCol(hstmt : 8 : SQL_VARCHAR : %addr(vc) : 22 : *null);
for cnt = 1 to 5;
  c2 = *all'*';
  c10 = *all'*';
  zn = -1;
  pk = -1;
  fl = -1;
  rc = SQLFetch(hstmt);
  say('fetch ' + %char(rc) + ' ' + c2 + ' ' + %char(l1) + ' [' + c10 + '] ' + %char(l2)
      + ' ' + %char(c10 = *blanks) + ' ' + %char(zn) + ' ' + %char(l3) + ' ' + %char(pk)
      + ' ' + %char(l4) + ' ' + %char(fl) + ' ' + %char(i4) + ' ' + %char(bi) + ' '
      + %char(si) + ' ' + vc);
  if rc = SQL_NO_DATA;
    leave;
  endif;
endfor;
rc = SQLCloseCursor(hstmt);
rc = SQLFreeStmt(hstmt : SQL_UNBIND);

// Scrolling a scrollable cursor, and one that is not.
v = 1;
rc = SQLSetStmtAttr(hstmt : SQL_ATTR_CURSOR_SCROLLABLE : %addr(v) : 0);
say('scrollable ' + %char(rc));
stmt = 'SELECT ID FROM ' + %trim(lib) + '.CLIT ORDER BY ID';
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
rc = SQLBindCol(hstmt : 1 : SQL_INTEGER : %addr(i4) : 4 : %addr(l1));
scroll(SQL_FETCH_LAST : 0);
scroll(SQL_FETCH_PRIOR : 0);
scroll(SQL_FETCH_FIRST : 0);
scroll(SQL_FETCH_ABSOLUTE : 3);
scroll(SQL_FETCH_RELATIVE : -1);
scroll(SQL_FETCH_NEXT : 0);
scroll(SQL_FETCH_ABSOLUTE : 9);
scroll(SQL_FETCH_PRIOR : 0);
rc = SQLCloseCursor(hstmt);
v = 0;
rc = SQLSetStmtAttr(hstmt : SQL_ATTR_CURSOR_SCROLLABLE : %addr(v) : 0);
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
scroll(SQL_FETCH_LAST : 0);
diag('forward-last');
scroll(9 : 0);
diag('orient-9');
rc = SQLCloseCursor(hstmt);
rc = SQLFreeStmt(hstmt : SQL_UNBIND);

// Changes under the default commitment control, then without it.
stmt = 'INSERT INTO ' + %trim(lib) + '.CLIT VALUES (9, ''Zed'', 1, NULL)';
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
say('insert-cmt ' + %char(rc));
diag('insert-cmt');
v = 1;
rc = SQLSetConnectAttr(hdbc : SQL_ATTR_COMMIT : %addr(v) : 0);
say('set-commit ' + %char(rc));
stmt = 'INSERT INTO ' + %trim(lib) + '.CLIT VALUES (?, ?, ?, NULL)';
rc = SQLAllocStmt(hdbc : %addr(h2));
rc = SQLPrepare(h2 : %addr(stmt) : %len(%trimr(stmt)));
rc = SQLExecute(h2);
say('execute-unbound ' + %char(rc));
rc = SQLGetDiagRec(SQL_HANDLE_STMT : h2 : 1 : %addr(state) : %addr(native) : %addr(msg) : 300
                   : %addr(mlen));
say('  execute-unbound diag ' + %char(rc) + ' ' + %subst(state : 1 : 5) + ' ' + %char(native));
rc = SQLFreeStmt(h2 : SQL_DROP);
rc = SQLPrepare(hstmt : %addr(stmt) : %len(%trimr(stmt)));
rc = SQLBindParam(hstmt : 1 : SQL_INTEGER : SQL_INTEGER : 0 : 0 : %addr(pid) : *null);
rc = SQLBindParam(hstmt : 2 : SQL_CHAR : SQL_VARCHAR : 20 : 0 : %addr(pname) : %addr(pnl));
rc = SQLBindParam(hstmt : 3 : SQL_DECIMAL : SQL_DECIMAL : 9 : 2 : %addr(pamt) : %addr(l2));
pid = 11;
pname = 'Nts' + x'00';
pnl = SQL_NTS;
pamt = 1.25;
l2 = 0;
rc = SQLExecute(hstmt);
say('insert-nts ' + %char(rc));
pid = 12;
pname = 'Length cut';
pnl = 6;
l2 = SQL_NULL_DATA;
rc = SQLExecute(hstmt);
say('insert-len ' + %char(rc));
rc = SQLRowCount(hstmt : %addr(cnt));
say('rowcount ' + %char(rc) + ' ' + %char(cnt));
rc = SQLFreeStmt(hstmt : SQL_RESET_PARAMS);
rc = SQLExecute(hstmt);
say('execute-reset ' + %char(rc));
diag('execute-reset');
stmt = 'UPDATE ' + %trim(lib) + '.CLIT SET AMT = AMT + 1 WHERE ID > 2';
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
rc = SQLRowCount(hstmt : %addr(cnt));
say('update ' + %char(rc) + ' ' + %char(cnt));
stmt = 'DELETE FROM ' + %trim(lib) + '.CLIT WHERE ID = 77';
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
say('delete-none ' + %char(rc));
rc = SQLRowCount(hstmt : %addr(cnt));
say('rowcount-none ' + %char(rc) + ' ' + %char(cnt));
stmt = 'SELECT ID, NAME, COALESCE(AMT, -1) FROM ' + %trim(lib) + '.CLIT WHERE ID > 2 ORDER BY ID';
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
rc = SQLRowCount(hstmt : %addr(cnt));
say('rowcount-select ' + %char(rc) + ' ' + %char(cnt));
rc = SQLBindCol(hstmt : 1 : SQL_INTEGER : %addr(i4) : 4 : *null);
rc = SQLBindCol(hstmt : 2 : SQL_CHAR : %addr(c20) : 20 : *null);
rc = SQLBindCol(hstmt : 3 : SQL_DECIMAL : %addr(pamt) : 9 * 256 + 2 : *null);
dow SQLFetch(hstmt) = 0;
  say('row ' + %char(i4) + ' ' + %trimr(c20) + ' ' + %char(pamt));
enddo;
rc = SQLCloseCursor(hstmt);
rc = SQLEndTran(SQL_HANDLE_DBC : hdbc : SQL_COMMIT);
say('endtran ' + %char(rc));
rc = SQLTransact(henv : 0 : SQL_ROLLBACK);
say('transact ' + %char(rc));
rc = SQLFreeStmt(hstmt : SQL_DROP);
rc = SQLDisconnect(hdbc);
rc = SQLFreeConnect(hdbc);
rc = SQLFreeEnv(henv);
*inlr = *on;

dcl-proc scroll;
  dcl-pi *n;
    o int(5) value;
    off int(10) value;
  end-pi;
  dcl-s r int(10);
  i4 = -1;
  r = SQLFetchScroll(hstmt : o : off);
  say('scroll ' + %char(o) + ' ' + %char(off) + ' ' + %char(r) + ' ' + %char(i4));
end-proc;

dcl-proc diag;
  dcl-pi *n;
    what varchar(30) const;
  end-pi;
  dcl-s r int(10);
  dcl-s rec int(5);
  for rec = 1 to 2;
    state = *all'*';
    native = -1;
    r = SQLGetDiagRec(SQL_HANDLE_STMT : hstmt : rec : %addr(state) : %addr(native)
                      : %addr(msg) : %size(msg) : %addr(mlen));
    if r <> 0;
      if rec = 1;
        say('  ' + what + ' diag ' + %char(r));
      endif;
      leave;
    endif;
    say('  ' + what + ' diag ' + %char(rec) + ' ' + %subst(state : 1 : 5) + ' ' + %char(native));
  endfor;
end-proc;

dcl-proc say;
  dcl-pi *n;
    text varchar(132) const;
  end-pi;
  line = text;
  write QSYSPRT line;
end-proc;
