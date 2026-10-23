/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* OVRDSPF: override a display file's name for the programs that run */
/* after it.                                                           */
             CMD        PROMPT('Override with Display File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(OVRDSPF)
             PARM       KWD(FILE) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*DSPF)) PROMPT('File being overridden')
             PARM       KWD(TOFILE) TYPE(Q1) DFT(*FILE) SNGVAL((*FILE)) +
                          PROMPT('Overriding to display file')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MAX(50) +
                          SNGVAL((*NONE) (*REQUESTER)) PMTCTL(*PMTRQS) +
                          PROMPT('Device')
             PARM       KWD(RSTDSP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Restore display')
             PARM       KWD(DFRWRT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Defer write')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Record format level check')
             PARM       KWD(SECURE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Secure from other overrides')
             PARM       KWD(OVRSCOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ACTGRPDFN) VALUES(*ACTGRPDFN *CALLLVL +
                          *JOB) PMTCTL(*PMTRQS) PROMPT('Override scope')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Share open data path')
             PARM       KWD(OPNSCOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          VALUES(*ACTGRPDFN *CALLLVL *JOB) PMTCTL(*PMTRQS) +
                          PROMPT('Open scope')
             PARM       KWD(WAITFILE) TYPE(*DEC) LEN(5) +
                          RANGE(1 32767) SPCVAL((*IMMED 0) (*CLS -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum file wait time')
             PARM       KWD(WAITRCD) TYPE(*DEC) LEN(5) +
                          RANGE(1 32767) SPCVAL((*NOMAX -1) (*IMMED 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum record wait time')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) SPCVAL((*LIBL) +
                          (*CURLIB)) PROMPT('Library')
