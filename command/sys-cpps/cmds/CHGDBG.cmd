/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGDBG: change the debug attributes of the job. */
             CMD        PROMPT('Change Debug') ALLOW(*ALL) +
                          HLPPNLGRP(QHDBGCMD) HLPID(CHGDBG)
             PARM       KWD(MAXTRC) TYPE(*INT4) DFT(*SAME) RANGE(0 99999) +
                          SPCVAL((*SAME -1)) PROMPT('Max trace statements')
             PARM       KWD(TRCFULL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *STOPTRC *WRAP) +
                          PROMPT('Trace full')
             PARM       KWD(UPDPROD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Update production files')
             PARM       KWD(OPMSRC) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('OPM source level debug')
