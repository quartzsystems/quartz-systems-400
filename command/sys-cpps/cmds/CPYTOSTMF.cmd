/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYTOSTMF: copy a database member's records into a stream file.    */
             CMD        PROMPT('Copy To Stream File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(CPYTOSTMF)
             PARM       KWD(FROMMBR) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Member path to copy')
             PARM       KWD(TOSTMF) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Stream file to write')
             PARM       KWD(STMFOPT) TYPE(*INT4) RSTD(*YES) DFT(*NONE) +
                          SPCVAL((*NONE 1) (*ADD 2) (*REPLACE 3)) +
                          PROMPT('When the stream file exists')
             PARM       KWD(CVTDTA) TYPE(*INT4) RSTD(*YES) DFT(*AUTO) +
                          SPCVAL((*AUTO 1) (*TBL 2) (*NONE 3)) +
                          PMTCTL(*PMTRQS) PROMPT('Data conversion')
             PARM       KWD(DBFCCSID) TYPE(*INT4) DFT(*FILE) +
                          RANGE(1 65533) SPCVAL((*FILE -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Member data CCSID')
             PARM       KWD(STMFCCSID) TYPE(*INT4) DFT(*STMF) +
                          RANGE(1 65533) SPCVAL((*STMF -1) (*PCASCII -2) +
                          (*STDASCII -3)) PMTCTL(*PMTRQS) +
                          PROMPT('Stream file data CCSID')
             PARM       KWD(TBL) TYPE(*PNAME) LEN(5000) PMTCTL(*PMTRQS) +
                          PROMPT('Conversion table path')
             PARM       KWD(ENDLINFMT) TYPE(*INT4) RSTD(*YES) +
                          DFT(*CRLF) SPCVAL((*CRLF 2) (*LF 3) (*CR 4) +
                          (*LFCR 5) (*FIXED 6)) PMTCTL(*PMTRQS) +
                          PROMPT('Line end written')
             PARM       KWD(AUT) TYPE(*INT4) RSTD(*YES) DFT(*DFT) +
                          SPCVAL((*DFT 1) (*INDIR 2) (*FILE 3) +
                          (*INDIRFILE 4)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority of a new file')
             PARM       KWD(STMFCODPAG) TYPE(*INT4) DFT(*STMF) +
                          RANGE(1 32767) SPCVAL((*STMF -1) (*PCASCII -2) +
                          (*STDASCII -3)) PMTCTL(*PMTRQS) +
                          PROMPT('Stream file code page')
