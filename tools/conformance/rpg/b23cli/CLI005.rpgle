**FREE
//; call: CALL PGM({lib}/{name}) PARM('{lib}')
//; with: CLIH
//; sqlfile: CLIT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/copy CLIH
// Character columns without the NUL (SQL_ATTR_OUTPUT_NTS off).
dcl-pi *n;
  lib char(10);
end-pi;
dcl-s henv int(10);
dcl-s hdbc int(10);
dcl-s hstmt int(10);
dcl-s rc int(10);
dcl-s v int(10);
dcl-s olen int(10);
dcl-s dsn char(10) inz('*LOCAL');
dcl-s stmt char(300);
dcl-s c2 char(2);
dcl-s c10 char(10);
dcl-s c20 char(20);
dcl-s l1 int(10);
dcl-s l2 int(10);
dcl-s l3 int(10);
dcl-s cnt int(10);

rc = SQLAllocEnv(%addr(henv));
v = 0;
rc = SQLSetEnvAttr(henv : SQL_ATTR_OUTPUT_NTS : %addr(v) : 0);
say('nts-off ' + %char(rc));
v = -9;
rc = SQLGetEnvAttr(henv : SQL_ATTR_OUTPUT_NTS : %addr(v) : 4 : %addr(olen));
say('nts ' + %char(rc) + ' ' + %char(v));
rc = SQLAllocConnect(henv : %addr(hdbc));
rc = SQLConnect(hdbc : %addr(dsn) : 6 : *null : 0 : *null : 0);
rc = SQLAllocStmt(hdbc : %addr(hstmt));
stmt = 'SELECT NAME, D, NAME FROM ' + %trim(lib) + '.CLIT ORDER BY ID';
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
rc = SQLBindCol(hstmt : 1 : SQL_CHAR : %addr(c2) : 2 : %addr(l1));
rc = SQLBindCol(hstmt : 2 : SQL_CHAR : %addr(c10) : 10 : %addr(l2));
rc = SQLBindCol(hstmt : 3 : SQL_CHAR : %addr(c20) : 20 : %addr(l3));
for cnt = 1 to 5;
  c2 = *all'*';
  c10 = *all'*';
  c20 = *all'*';
  rc = SQLFetch(hstmt);
  say('fetch ' + %char(rc) + ' ' + c2 + ' ' + %char(l1) + ' [' + c10 + '] ' + %char(l2)
      + ' [' + c20 + '] ' + %char(l3));
  if rc = SQL_NO_DATA;
    leave;
  endif;
endfor;
rc = SQLCloseCursor(hstmt);
rc = SQLFreeStmt(hstmt : SQL_DROP);
rc = SQLDisconnect(hdbc);
rc = SQLFreeConnect(hdbc);
rc = SQLFreeEnv(henv);
*inlr = *on;

dcl-proc say;
  dcl-pi *n;
    text varchar(132) const;
  end-pi;
  line = text;
  write QSYSPRT line;
end-proc;
