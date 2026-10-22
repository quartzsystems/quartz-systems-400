/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* OVRMSGF: have another message file looked at first. */
             CMD        PROMPT('Override with Message File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(OVRMSGF)
             PARM       KWD(MSGF) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Message file being overridden')
             PARM       KWD(TOMSGF) TYPE(Q1) MIN(1) +
                          PROMPT('Overriding to message file')
             PARM       KWD(SECURE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Secure from other overrides')
             PARM       KWD(OVRSCOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ACTGRPDFN) VALUES(*ACTGRPDFN *CALLLVL +
                          *JOB) PMTCTL(*PMTRQS) PROMPT('Override scope')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
