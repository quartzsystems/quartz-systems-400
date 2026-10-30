/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGENVVAR: change an environment variable of the job or the system. */
             CMD        PROMPT('Change Environment Variable') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHP0ZCMD) +
                          HLPID(CHGENVVAR)
             PARM       KWD(ENVVAR) TYPE(*CHAR) LEN(128) MIN(1) +
                          CASE(*MIXED) PROMPT('Environment variable')
             PARM       KWD(VALUE) TYPE(*CHAR) LEN(1024) DFT(*SAME) +
                          SPCVAL((*SAME) (*NULL)) CASE(*MIXED) +
                          PMTCTL(PV) PROMPT('New value')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*SAME) RANGE(1 +
                          65535) SPCVAL((*JOB 0) (*HEX 65535) (*SAME +
                          -1)) EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Coded character set ID')
             PARM       KWD(LEVEL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *SYS) PMTCTL(*PMTRQS) +
                          PROMPT('Level')
 PV:         PMTCTL     CTL(ENVVAR) COND((*SPCFD))
