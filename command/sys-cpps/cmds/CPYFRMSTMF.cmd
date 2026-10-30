/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYFRMSTMF: copy a stream file's lines into a database member.     */
             CMD        PROMPT('Copy From Stream File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(CPYFRMSTMF)
             PARM       KWD(FROMSTMF) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Stream file to read')
             PARM       KWD(TOMBR) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Member path to write')
             PARM       KWD(MBROPT) TYPE(*INT4) RSTD(*YES) DFT(*NONE) +
                          SPCVAL((*NONE 1) (*ADD 2) (*REPLACE 3)) +
                          PROMPT('When the member exists')
             PARM       KWD(CVTDTA) TYPE(*INT4) RSTD(*YES) DFT(*AUTO) +
                          SPCVAL((*AUTO 1) (*TBL 2) (*NONE 3)) +
                          PMTCTL(*PMTRQS) PROMPT('Data conversion')
             PARM       KWD(STMFCCSID) TYPE(*INT4) DFT(*STMF) +
                          RANGE(1 65533) SPCVAL((*STMF -1) (*PCASCII -2)) +
                          PMTCTL(*PMTRQS) PROMPT('Stream file data CCSID')
             PARM       KWD(DBFCCSID) TYPE(*INT4) DFT(*FILE) +
                          RANGE(1 65533) SPCVAL((*FILE -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Member data CCSID')
             PARM       KWD(TBL) TYPE(*PNAME) LEN(5000) PMTCTL(*PMTRQS) +
                          PROMPT('Conversion table path')
             PARM       KWD(ENDLINFMT) TYPE(*INT4) RSTD(*YES) DFT(*ALL) +
                          SPCVAL((*ALL 1) (*CRLF 2) (*LF 3) (*CR 4) +
                          (*LFCR 5) (*FIXED 6)) PMTCTL(*PMTRQS) +
                          PROMPT('Line ends read')
             PARM       KWD(TABEXPN) TYPE(*INT4) RSTD(*YES) DFT(*YES) +
                          SPCVAL((*YES 1) (*NO 2)) PMTCTL(*PMTRQS) +
                          PROMPT('Tabs to blanks')
             PARM       KWD(STMFCODPAG) TYPE(*INT4) DFT(*STMF) +
                          RANGE(1 32767) SPCVAL((*STMF -1) (*PCASCII -2)) +
                          PMTCTL(*PMTRQS) PROMPT('Stream file code page')
