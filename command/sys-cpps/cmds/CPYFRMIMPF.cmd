/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYFRMIMPF: copy a delimited or fixed-format import file or stream */
/* file into a database member.                                        */
             CMD        PROMPT('Copy From Import File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CPYFRMIMPF)
             PARM       KWD(FROMFILE) TYPE(Q1) PROMPT('From file')
             PARM       KWD(FROMSTMF) TYPE(*PNAME) LEN(5000) +
                          PROMPT('From stream file')
             PARM       KWD(TOFILE) TYPE(Q1) MIN(1) PROMPT('To file')
             PARM       KWD(MBROPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ADD *REPLACE) +
                          PROMPT('Replace or add records')
             PARM       KWD(FROMCCSID) TYPE(*DEC) LEN(5) DFT(*FILE) +
                          RANGE(1 65533) SPCVAL((*FILE 0)) +
                          PROMPT('From CCSID')
             PARM       KWD(TOCCSID) TYPE(*DEC) LEN(5) DFT(*FILE) +
                          RANGE(1 65533) SPCVAL((*FILE 0)) +
                          PROMPT('To CCSID')
             PARM       KWD(RCDDLM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*EOR) VALUES(*EOR *ALL *CRLF *LF *CR +
                          *LFCR) PROMPT('Record delimiter')
             PARM       KWD(DTAFMT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*DLM) VALUES(*DLM *FIXED) +
                          PROMPT('Data format')
             PARM       KWD(STRDLM) TYPE(*CHAR) LEN(1) DFT(*DBLQUOTE) +
                          SPCVAL((*DBLQUOTE '"') (*NONE ' ')) +
                          PROMPT('String delimiter')
             PARM       KWD(STRESCCHR) TYPE(*CHAR) LEN(1) DFT(*STRDLM) +
                          SPCVAL((*STRDLM ' ') (*NONE ' ')) +
                          PROMPT('String escape character')
             PARM       KWD(RMVBLANK) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*TRAILING) VALUES(*TRAILING *NONE +
                          *LEADING *BOTH) PROMPT('Remove blanks')
             PARM       KWD(FLDDLM) TYPE(*CHAR) LEN(1) DFT(',') +
                          SPCVAL((*TAB ' ')) PROMPT('Field delimiter')
             PARM       KWD(DECPNT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*PERIOD) VALUES(*PERIOD *COMMA) +
                          PROMPT('Decimal point')
             PARM       KWD(DATFMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*ISO) VALUES(*ISO *USA *EUR *JIS *MDY +
                          *DMY *YMD *JUL *YYMD *MDYY *DMYY) +
                          PROMPT('Date format')
             PARM       KWD(DATSEP) TYPE(*CHAR) LEN(1) DFT('-') +
                          SPCVAL((*BLANK ' ')) PROMPT('Date separator')
             PARM       KWD(TIMFMT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*ISO) VALUES(*ISO *USA *EUR *JIS *HMS) +
                          PROMPT('Time format')
             PARM       KWD(TIMSEP) TYPE(*CHAR) LEN(1) DFT(':') +
                          SPCVAL((*BLANK ' ')) PROMPT('Time separator')
             PARM       KWD(FLDDFNFILE) TYPE(Q1) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Field definition file')
             PARM       KWD(ERRRCDFILE) TYPE(Q1) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Error record file')
             PARM       KWD(ERRRCDOPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add error records')
             PARM       KWD(RPLNULLVAL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *FLDDFT) +
                          PROMPT('Replace null values')
             PARM       KWD(FROMRCD) TYPE(*DEC) LEN(10) DFT(*FIRST) +
                          RANGE(1 4294967288) SPCVAL((*FIRST 0)) +
                          PROMPT('From record number')
             PARM       KWD(TORCD) TYPE(*DEC) LEN(10) DFT(*END) +
                          RANGE(1 4294967288) SPCVAL((*END 0)) +
                          PROMPT('To record number')
             PARM       KWD(ERRLVL) TYPE(*DEC) LEN(10) DFT(0) +
                          RANGE(0 4294967288) SPCVAL((*NOMAX 4294967289)) +
                          PROMPT('Errors allowed')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
