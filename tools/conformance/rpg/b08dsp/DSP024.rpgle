**FREE
//; dds: B8X
//; keys: @3,13 enter
//; keys: @4,12 f4
//; keys: @10,40 enter
//; keys: f3
//; keys: @11,15 enter
//; keys: enter
ctl-opt dftactgrp(*no);
dcl-f B8X workstn;
// RTNCSRLOC: the record, field and position under the cursor, after
// Enter and a function key, blanks on no field; *WINDOW the row and
// column in the window.
exfmt DETAIL;
SEEN = 'enter ' + CSRRCD + ' ' + CSRFLD + ' ' + %char(CSRPOS);
exfmt DETAIL;
SEEN = 'f4 ' + CSRRCD + ' ' + CSRFLD + ' ' + %char(CSRPOS);
exfmt DETAIL;
SEEN = 'none [' + CSRRCD + '][' + CSRFLD + '] ' + %char(CSRPOS);
exfmt DETAIL;
exfmt WIN;
WSEEN = 'window ' + %char(WROW) + ',' + %char(WCOL);
exfmt WIN;
*inlr = *on;
