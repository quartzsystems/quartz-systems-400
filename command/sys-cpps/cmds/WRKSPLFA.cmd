/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSPLFA: show a spooled file's attributes. */
             CMD        PROMPT('Work with Spooled File Attributes') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHSPCMD) +
                          HLPID(WRKSPLFA)
             PARM       KWD(FILE) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Spooled file')
             PARM       KWD(JOB) TYPE(QJ) DFT(*) SNGVAL((*)) +
                          PROMPT('Job name')
             PARM       KWD(SPLNBR) TYPE(*INT4) DFT(*ONLY) +
                          RANGE(1 999999) SPCVAL((*ONLY 0) (*LAST -1)) +
                          PROMPT('Spooled file number')
             PARM       KWD(JOBSYSNAME) TYPE(*CHAR) LEN(8) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*CURRENT) (*ANY)) +
                          PROMPT('Job system name')
             PARM       KWD(CRTDATE) TYPE(EC) DFT(*ONLY) +
                          SNGVAL((*ONLY) (*LAST)) +
                          PROMPT('Spooled file created')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             QJ:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
             EC:        ELEM       TYPE(*DATE) PROMPT('Creation date')
                        ELEM       TYPE(*TIME) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*LAST)) PROMPT('Creation time')
