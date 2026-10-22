/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDWSE: add a workstation entry, by name or by type, to a */
/* subsystem description.                                       */
             CMD        PROMPT('Add Work Station Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(ADDWSE)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(WRKSTN) TYPE(*GENERIC) LEN(10) +
                          PROMPT('Work station name')
             PARM       KWD(WRKSTNTYPE) TYPE(*CHAR) LEN(10) +
                          SPCVAL((*ALL) (*NONASCII) (*ASCII)) +
                          PROMPT('Work station type')
             PARM       KWD(JOBD) TYPE(Q2) DFT(*USRPRF) +
                          SNGVAL((*USRPRF) (*SBSD)) PROMPT('Job description')
             PARM       KWD(MAXACT) TYPE(*INT4) DFT(*NOMAX) +
                          RANGE(1 1000) SPCVAL((*NOMAX -1)) +
                          PROMPT('Maximum active jobs')
             PARM       KWD(AT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SIGNON) VALUES(*SIGNON *ENTER) +
                          PROMPT('Allocation')
             DEP        CTL(*ALWAYS) PARM(&WRKSTN &WRKSTNTYPE) +
                          NBRTRUE((*EQ 1))
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
