**free
// RLATEST: the QSTEST record-level script on IBM i.
// Generated from rla/qstest.rla by `cargo test -p conformance`; do not edit.
// Every script line logs LINE, %status, F/E/O flags and the fields it names
// into QSTEST/RLALOG; the comparer reads that back as rla.csv.

ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt:*nodebugio) alwnull(*usrctl);

dcl-f ACTIVE disk(*ext) usage(*input) usropn keyed rename(CUSREC:CUSREC3);
dcl-f BIGORD disk(*ext) usage(*input) usropn keyed rename(ORDREC:ORDREC2);
dcl-f CUSBYNAME disk(*ext) usage(*input) usropn keyed rename(CUSREC:CUSREC2);
dcl-f CUSTOMER disk(*ext) usage(*input:*update:*delete:*output) usropn keyed;
dcl-f ORDERS disk(*ext) usage(*input) usropn;
dcl-f RLALOG disk(*ext) usage(*output) prefix(LG_);

dcl-s detail varchar(200);

// 3: OPEN QSTEST/CUSTOMER ALL KEYED         => 00000
open(e) CUSTOMER;
detail = '';
logline(3 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 4: SETLL 3                                => 00000 FOUND EQUAL
setll(e) 3 CUSTOMER;
detail = '';
logline(4 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 5: READ                                   => 00000 CUSNO=3 NAME='Carol'
read(e) CUSTOMER;
detail = '';
detail += 'CUSNO=' + %char(CUSNO) + ' ';
detail += 'NAME=' + %trimr(NAME) + ' ';
logline(5 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 6: READ                                   => 00000 CUSNO=4
read(e) CUSTOMER;
detail = '';
detail += 'CUSNO=' + %char(CUSNO) + ' ';
logline(6 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 7: SETGT 4                                => 00000 FOUND
setgt(e) 4 CUSTOMER;
detail = '';
logline(7 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 8: READ                                   => 00000 CUSNO=5
read(e) CUSTOMER;
detail = '';
detail += 'CUSNO=' + %char(CUSNO) + ' ';
logline(8 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 9: CHAIN 42                               => 00012 NOFOUND
chain(e) 42 CUSTOMER;
detail = '';
logline(9 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 10: CHAIN 6                                => 00000 CUSNO=6 NAME='Frank' BAL=1500.00
chain(e) 6 CUSTOMER;
detail = '';
detail += 'CUSNO=' + %char(CUSNO) + ' ';
detail += 'NAME=' + %trimr(NAME) + ' ';
detail += 'BAL=' + %char(BAL) + ' ';
logline(10 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 11: READP                                  => 00000 CUSNO=5
readp(e) CUSTOMER;
detail = '';
detail += 'CUSNO=' + %char(CUSNO) + ' ';
logline(11 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 12: SETLL *END                             => 00000
setll(e) *hival CUSTOMER;
detail = '';
logline(12 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 13: READP                                  => 00000 CUSNO=6
readp(e) CUSTOMER;
detail = '';
detail += 'CUSNO=' + %char(CUSNO) + ' ';
logline(13 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 14: READ                                   => 00011 EOF
read(e) CUSTOMER;
detail = '';
logline(14 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 15: SETLL 5                                => 00000 FOUND EQUAL
setll(e) 5 CUSTOMER;
detail = '';
logline(15 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 16: READE 5                                => 00000 CUSNO=5
reade(e) 5 CUSTOMER;
detail = '';
detail += 'CUSNO=' + %char(CUSNO) + ' ';
logline(16 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 17: READE 5                                => 00011 EOF
reade(e) 5 CUSTOMER;
detail = '';
logline(17 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 18: READ                                   => 00000 CUSNO=6
read(e) CUSTOMER;
detail = '';
detail += 'CUSNO=' + %char(CUSNO) + ' ';
logline(18 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 19: WRITE CUSNO=7 NAME='Grace' STATUS='A' BAL=5 => 00000
clear CUSREC;
%nullind(SINCE) = *on;
%nullind(NOTE) = *on;
CUSNO = 7;
NAME = 'Grace';
STATUS = 'A';
BAL = 5;
write(e) CUSREC;
detail = '';
logline(19 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 20: CHAIN 7                                => 00000 NAME='Grace' SINCE=*NULL
chain(e) 7 CUSTOMER;
detail = '';
detail += 'NAME=' + %trimr(NAME) + ' ';
if %nullind(SINCE);
  detail += 'SINCE=*NULL ';
else;
  detail += 'SINCE=' + %char(SINCE) + ' ';
endif;
logline(20 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 21: UPDATE BAL=6.5 NOTE='updated'          => 00000
BAL = 6.5;
%nullind(NOTE) = *off;
NOTE = 'updated';
update(e) CUSREC;
detail = '';
logline(21 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 22: CHAINN 7                               => 00000 BAL=6.50 NOTE='updated'
chain(en) 7 CUSTOMER;
detail = '';
detail += 'BAL=' + %char(BAL) + ' ';
if %nullind(NOTE);
  detail += 'NOTE=*NULL ';
else;
  detail += 'NOTE=' + %trimr(NOTE) + ' ';
endif;
logline(22 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 23: UPDATE BAL=1                           => 01221
BAL = 1;
update(e) CUSREC;
detail = '';
logline(23 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 24: CHAIN 7                                => 00000
chain(e) 7 CUSTOMER;
detail = '';
logline(24 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 25: DELETE                                 => 00000
delete(e) CUSREC;
detail = '';
logline(25 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 26: CHAIN 7                                => 00012 NOFOUND
chain(e) 7 CUSTOMER;
detail = '';
logline(26 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 27: WRITE CUSNO=1 NAME='dup' STATUS='A'    => 01021
clear CUSREC;
%nullind(SINCE) = *on;
%nullind(NOTE) = *on;
CUSNO = 1;
NAME = 'dup';
STATUS = 'A';
write(e) CUSREC;
detail = '';
logline(27 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 28: UNLOCK                                 => 00000
unlock(e) CUSTOMER;
detail = '';
logline(28 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 29: CLOSE                                  => 00000
close(e) CUSTOMER;
detail = '';
logline(29 : %status(CUSTOMER) : %found(CUSTOMER) : %equal(CUSTOMER) : %eof(CUSTOMER) : %trimr(detail));
// 30: OPEN QSTEST/CUSBYNAME INPUT KEYED      => 00000
open(e) CUSBYNAME;
detail = '';
logline(30 : %status(CUSBYNAME) : %found(CUSBYNAME) : %equal(CUSBYNAME) : %eof(CUSBYNAME) : %trimr(detail));
// 31: READ                                   => 00000 NAME='Alice' CUSNO=1
read(e) CUSBYNAME;
detail = '';
detail += 'NAME=? ';
detail += 'CUSNO=? ';
logline(31 : %status(CUSBYNAME) : %found(CUSBYNAME) : %equal(CUSBYNAME) : %eof(CUSBYNAME) : %trimr(detail));
// 32: READE 'Alice'                          => 00011 EOF
reade(e) 'Alice' CUSBYNAME;
detail = '';
logline(32 : %status(CUSBYNAME) : %found(CUSBYNAME) : %equal(CUSBYNAME) : %eof(CUSBYNAME) : %trimr(detail));
// 33: SETLL 'Carol'                          => 00000 FOUND EQUAL
setll(e) 'Carol' CUSBYNAME;
detail = '';
logline(33 : %status(CUSBYNAME) : %found(CUSBYNAME) : %equal(CUSBYNAME) : %eof(CUSBYNAME) : %trimr(detail));
// 34: READE 'Carol'                          => 00000 CUSNO=3
reade(e) 'Carol' CUSBYNAME;
detail = '';
detail += 'CUSNO=? ';
logline(34 : %status(CUSBYNAME) : %found(CUSBYNAME) : %equal(CUSBYNAME) : %eof(CUSBYNAME) : %trimr(detail));
// 35: READE 'Carol'                          => 00011 EOF
reade(e) 'Carol' CUSBYNAME;
detail = '';
logline(35 : %status(CUSBYNAME) : %found(CUSBYNAME) : %equal(CUSBYNAME) : %eof(CUSBYNAME) : %trimr(detail));
// 36: SETGT 'Dave'                           => 00000 FOUND
setgt(e) 'Dave' CUSBYNAME;
detail = '';
logline(36 : %status(CUSBYNAME) : %found(CUSBYNAME) : %equal(CUSBYNAME) : %eof(CUSBYNAME) : %trimr(detail));
// 37: READ                                   => 00000 NAME='Erin'
read(e) CUSBYNAME;
detail = '';
detail += 'NAME=? ';
logline(37 : %status(CUSBYNAME) : %found(CUSBYNAME) : %equal(CUSBYNAME) : %eof(CUSBYNAME) : %trimr(detail));
// 38: CLOSE                                  => 00000
close(e) CUSBYNAME;
detail = '';
logline(38 : %status(CUSBYNAME) : %found(CUSBYNAME) : %equal(CUSBYNAME) : %eof(CUSBYNAME) : %trimr(detail));
// 39: OPEN QSTEST/ORDERS INPUT ARRIVAL       => 00000
open(e) ORDERS;
detail = '';
logline(39 : %status(ORDERS) : %found(ORDERS) : %equal(ORDERS) : %eof(ORDERS) : %trimr(detail));
// 40: CHAIN #3                               => 00000 ORDNO=1003 AMOUNT=1000.00
chain(e) 3 ORDERS;
detail = '';
detail += 'ORDNO=' + %char(ORDNO) + ' ';
detail += 'AMOUNT=' + %char(AMOUNT) + ' ';
logline(40 : %status(ORDERS) : %found(ORDERS) : %equal(ORDERS) : %eof(ORDERS) : %trimr(detail));
// 41: READ                                   => 00000 ORDNO=1004
read(e) ORDERS;
detail = '';
detail += 'ORDNO=' + %char(ORDNO) + ' ';
logline(41 : %status(ORDERS) : %found(ORDERS) : %equal(ORDERS) : %eof(ORDERS) : %trimr(detail));
// 42: CHAIN #9                               => 00012 NOFOUND
chain(e) 9 ORDERS;
detail = '';
logline(42 : %status(ORDERS) : %found(ORDERS) : %equal(ORDERS) : %eof(ORDERS) : %trimr(detail));
// 43: CLOSE                                  => 00000
close(e) ORDERS;
detail = '';
logline(43 : %status(ORDERS) : %found(ORDERS) : %equal(ORDERS) : %eof(ORDERS) : %trimr(detail));
// 44: OPEN QSTEST/ACTIVE INPUT KEYED         => 00000
open(e) ACTIVE;
detail = '';
logline(44 : %status(ACTIVE) : %found(ACTIVE) : %equal(ACTIVE) : %eof(ACTIVE) : %trimr(detail));
// 45: READ                                   => 00000 CUSNO=1
read(e) ACTIVE;
detail = '';
detail += 'CUSNO=? ';
logline(45 : %status(ACTIVE) : %found(ACTIVE) : %equal(ACTIVE) : %eof(ACTIVE) : %trimr(detail));
// 46: READ                                   => 00000 CUSNO=3
read(e) ACTIVE;
detail = '';
detail += 'CUSNO=? ';
logline(46 : %status(ACTIVE) : %found(ACTIVE) : %equal(ACTIVE) : %eof(ACTIVE) : %trimr(detail));
// 47: READ                                   => 00000 CUSNO=4
read(e) ACTIVE;
detail = '';
detail += 'CUSNO=? ';
logline(47 : %status(ACTIVE) : %found(ACTIVE) : %equal(ACTIVE) : %eof(ACTIVE) : %trimr(detail));
// 48: READ                                   => 00000 CUSNO=6
read(e) ACTIVE;
detail = '';
detail += 'CUSNO=? ';
logline(48 : %status(ACTIVE) : %found(ACTIVE) : %equal(ACTIVE) : %eof(ACTIVE) : %trimr(detail));
// 49: READ                                   => 00011 EOF
read(e) ACTIVE;
detail = '';
logline(49 : %status(ACTIVE) : %found(ACTIVE) : %equal(ACTIVE) : %eof(ACTIVE) : %trimr(detail));
// 50: CLOSE                                  => 00000
close(e) ACTIVE;
detail = '';
logline(50 : %status(ACTIVE) : %found(ACTIVE) : %equal(ACTIVE) : %eof(ACTIVE) : %trimr(detail));
// 51: OPEN QSTEST/BIGORD INPUT KEYED         => 00000
open(e) BIGORD;
detail = '';
logline(51 : %status(BIGORD) : %found(BIGORD) : %equal(BIGORD) : %eof(BIGORD) : %trimr(detail));
// 52: READ                                   => 00000 ORDNO=1003
read(e) BIGORD;
detail = '';
detail += 'ORDNO=? ';
logline(52 : %status(BIGORD) : %found(BIGORD) : %equal(BIGORD) : %eof(BIGORD) : %trimr(detail));
// 53: READ                                   => 00000 ORDNO=1004
read(e) BIGORD;
detail = '';
detail += 'ORDNO=? ';
logline(53 : %status(BIGORD) : %found(BIGORD) : %equal(BIGORD) : %eof(BIGORD) : %trimr(detail));
// 54: READ                                   => 00011 EOF
read(e) BIGORD;
detail = '';
logline(54 : %status(BIGORD) : %found(BIGORD) : %equal(BIGORD) : %eof(BIGORD) : %trimr(detail));
// 55: CLOSE                                  => 00000
close(e) BIGORD;
detail = '';
logline(55 : %status(BIGORD) : %found(BIGORD) : %equal(BIGORD) : %eof(BIGORD) : %trimr(detail));

*inlr = *on;
return;

dcl-proc logline;
  dcl-pi *n;
    line packed(4:0) const;
    status packed(5:0) const;
    fnd ind const;
    eq ind const;
    eof ind const;
    det varchar(200) const;
  end-pi;
  LG_LINE = line;
  LG_STATUS = %editc(status:'X');
  LG_FLAGS = '---';
  if fnd;
    %subst(LG_FLAGS:1:1) = 'F';
  endif;
  if eq;
    %subst(LG_FLAGS:2:1) = 'E';
  endif;
  if eof;
    %subst(LG_FLAGS:3:1) = 'O';
  endif;
  LG_DETAIL = det;
  write LOGREC;
end-proc;
