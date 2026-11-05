**FREE
//; call: CALL PGM({lib}/{name}) PARM('{lib}')
//; with: CLIH
//; sqlfile: CLIT
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/copy CLIH
// Server mode: a prepared statement with markers, bound parameters
// and columns, executed and fetched; then the diagnostics of an error.
dcl-pi *n;
  lib char(10);
end-pi;
dcl-s henv int(10);
dcl-s hdbc int(10);
dcl-s hstmt int(10);
dcl-s rc int(10);
dcl-s on int(10) inz(1);
dcl-s v int(10);
dcl-s olen int(10);
dcl-s dsn char(10) inz('*LOCAL');
dcl-s stmt char(200);
dcl-s minid int(10);
dcl-s maxamt packed(9:2);
dcl-s id int(10);
dcl-s name char(20);
dcl-s amt packed(9:2);
dcl-s d char(10);
dcl-s idl int(10);
dcl-s namel int(10);
dcl-s amtl int(10);
dcl-s dl int(10);
dcl-s n int(5);
dcl-s cnt int(10);
dcl-s state char(6);
dcl-s native int(10);
dcl-s msg char(200);
dcl-s mlen int(5);
dcl-s job char(28);
dcl-s jobl int(10);

rc = SQLAllocEnv(%addr(henv));
say('allocenv ' + %char(rc) + ' ' + %char(henv));
rc = SQLSetEnvAttr(henv : SQL_ATTR_SERVER_MODE : %addr(on) : 0);
say('servermode ' + %char(rc));
v = -9;
rc = SQLGetEnvAttr(henv : SQL_ATTR_SERVER_MODE : %addr(v) : 4 : %addr(olen));
say('getenv ' + %char(rc) + ' ' + %char(v));
rc = SQLAllocConnect(henv : %addr(hdbc));
say('allocconnect ' + %char(rc) + ' ' + %char(hdbc));
rc = SQLConnect(hdbc : %addr(dsn) : 6 : *null : 0 : *null : 0);
say('connect ' + %char(rc));
v = -9;
rc = SQLGetConnectAttr(hdbc : SQL_ATTR_AUTOCOMMIT : %addr(v) : 4 : %addr(olen));
say('autocommit ' + %char(rc) + ' ' + %char(v));
v = -9;
rc = SQLGetConnectAttr(hdbc : SQL_ATTR_COMMIT : %addr(v) : 4 : %addr(olen));
say('commit ' + %char(rc) + ' ' + %char(v));
v = -9;
rc = SQLGetConnectAttr(hdbc : SQL_ATTR_DBC_SYS_NAMING : %addr(v) : 4 : %addr(olen));
say('sysnaming ' + %char(rc) + ' ' + %char(v));
rc = SQLAllocStmt(hdbc : %addr(hstmt));
say('allocstmt ' + %char(rc) + ' ' + %char(hstmt));

// The job that runs the connection's statements.
stmt = 'SELECT SUBSTR(QSYS2.JOB_NAME, LOCATE(''/'', QSYS2.JOB_NAME) + 1), '
     + 'CASE WHEN QSYS2.SERVER_MODE_JOB_NAME IS NULL THEN ''none'' ELSE ''set'' END '
     + 'FROM SYSIBM.SYSDUMMY1';
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
say('execdirect ' + %char(rc));
rc = SQLBindCol(hstmt : 1 : SQL_CHAR : %addr(job) : %size(job) : %addr(jobl));
rc = SQLBindCol(hstmt : 2 : SQL_CHAR : %addr(d) : %size(d) : %addr(dl));
rc = SQLFetch(hstmt);
say('job ' + %char(rc) + ' ' + %trimr(job) + ' ' + %char(jobl) + ' ' + %trimr(d) + ' ' + %char(dl));
rc = SQLCloseCursor(hstmt);
say('close ' + %char(rc));
rc = SQLFreeStmt(hstmt : SQL_UNBIND);

// A prepared statement with two markers.
stmt = 'SELECT ID, NAME, AMT, D FROM ' + %trim(lib)
     + '.CLIT WHERE ID >= ? AND (AMT < ? OR AMT IS NULL) ORDER BY ID';
rc = SQLPrepare(hstmt : %addr(stmt) : %len(%trimr(stmt)));
say('prepare ' + %char(rc));
rc = SQLNumParams(hstmt : %addr(n));
say('numparams ' + %char(rc) + ' ' + %char(n));
rc = SQLNumResultCols(hstmt : %addr(n));
say('numcols ' + %char(rc) + ' ' + %char(n));
rc = SQLBindParam(hstmt : 1 : SQL_INTEGER : SQL_INTEGER : 0 : 0 : %addr(minid) : *null);
say('bindparam1 ' + %char(rc));
rc = SQLBindParam(hstmt : 2 : SQL_DECIMAL : SQL_DECIMAL : 9 : 2 : %addr(maxamt) : *null);
say('bindparam2 ' + %char(rc));
rc = SQLBindCol(hstmt : 1 : SQL_INTEGER : %addr(id) : 4 : %addr(idl));
rc = SQLBindCol(hstmt : 2 : SQL_CHAR : %addr(name) : %size(name) : %addr(namel));
rc = SQLBindCol(hstmt : 3 : SQL_DECIMAL : %addr(amt) : %len(amt) * 256 + %decpos(amt)
                : %addr(amtl));
rc = SQLBindCol(hstmt : 4 : SQL_CHAR : %addr(d) : %size(d) : %addr(dl));
say('bindcol ' + %char(rc));
minid = 1;
maxamt = 11;
exsr run;
// The same statement again with new values in the bound storage.
minid = 2;
maxamt = 0;
exsr run;

// An undefined table: the diagnostics.
stmt = 'SELECT * FROM ' + %trim(lib) + '.NOSUCHT';
rc = SQLExecDirect(hstmt : %addr(stmt) : %len(%trimr(stmt)));
say('bad ' + %char(rc));
rc = SQLError(henv : hdbc : hstmt : %addr(state) : %addr(native) : %addr(msg) : %size(msg)
              : %addr(mlen));
say('error ' + %char(rc) + ' ' + %subst(state : 1 : 5) + ' ' + %char(native));
rc = SQLError(henv : hdbc : hstmt : %addr(state) : %addr(native) : %addr(msg) : %size(msg)
              : %addr(mlen));
say('error2 ' + %char(rc));

rc = SQLFreeStmt(hstmt : SQL_DROP);
say('freestmt ' + %char(rc));
rc = SQLDisconnect(hdbc);
say('disconnect ' + %char(rc));
rc = SQLFreeConnect(hdbc);
say('freeconnect ' + %char(rc));
rc = SQLFreeEnv(henv);
say('freeenv ' + %char(rc));
*inlr = *on;

begsr run;
  rc = SQLExecute(hstmt);
  say('execute ' + %char(rc));
  cnt = 0;
  dow cnt < 10;
    id = -1;
    name = *all'*';
    amt = -1;
    d = *all'*';
    rc = SQLFetch(hstmt);
    if rc <> SQL_SUCCESS;
      leave;
    endif;
    cnt += 1;
    say('row ' + %char(id) + ' ' + %char(idl) + ' ' + %trimr(name) + ' ' + %char(namel)
        + ' ' + %char(amt) + ' ' + %char(amtl) + ' ' + %trimr(d) + ' ' + %char(dl));
  enddo;
  say('fetch ' + %char(rc) + ' ' + %char(cnt));
  rc = SQLCloseCursor(hstmt);
  say('close ' + %char(rc));
endsr;

dcl-proc say;
  dcl-pi *n;
    text varchar(132) const;
  end-pi;
  line = text;
  write QSYSPRT line;
end-proc;
