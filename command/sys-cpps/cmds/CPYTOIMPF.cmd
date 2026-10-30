/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYTOIMPF: copy a database member to a delimited or fixed-format */
/* import file or stream file.                                        */
             CMD        PROMPT('Copy To Import File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CPYTOIMPF)
             PARM       KWD(FROMFILE) TYPE(Q1) MIN(1) PROMPT('From file')
             PARM       KWD(TOFILE) TYPE(Q1) PROMPT('To file')
             PARM       KWD(TOSTMF) TYPE(*PNAME) LEN(5000) +
                          PROMPT('To stream file')
             PARM       KWD(MBROPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ADD *REPLACE) +
                          PROMPT('Replace or add records')
             PARM       KWD(STMFOPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ADD *REPLACE) +
                          PROMPT('Replace or add to stream file')
             PARM       KWD(FROMCCSID) TYPE(*DEC) LEN(5) DFT(*FILE) +
                          RANGE(1 65533) SPCVAL((*FILE 0)) +
                          PROMPT('From CCSID')
             PARM       KWD(STMFCCSID) TYPE(*DEC) LEN(5) DFT(*STMF) +
                          RANGE(1 65533) SPCVAL((*STMF -1) (*PCASCII -2) +
                          (*STDASCII -3)) PROMPT('Stream file CCSID')
             PARM       KWD(STMFCODPAG) TYPE(*DEC) LEN(5) DFT(*STMF) +
                          RANGE(1 32767) SPCVAL((*STMF -1) (*PCASCII -2) +
                          (*STDASCII -3)) PMTCTL(*PMTRQS) +
                          PROMPT('Stream file code page')
             PARM       KWD(RCDDLM) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*EOR) VALUES(*EOR *ALL *CRLF *LF *CR +
                          *LFCR *FIXED) PROMPT('Record delimiter')
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
             PARM       KWD(ADDCOLNAM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *SQL *SYS) +
                          PROMPT('Add column names')
             PARM       KWD(ORDERBY) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ARRIVAL) +
                          PROMPT('Order by')
             PARM       KWD(NULLS) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Write null values')
             PARM       KWD(STMFAUT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*DFT) VALUES(*DFT *INDIR *FILE *INDIRFILE) +
                          PROMPT('Stream file authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
