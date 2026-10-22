/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPOVR: display the file overrides in effect. */
             CMD        PROMPT('Display Override') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(DSPOVR)
             PARM       KWD(FILE) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('File')
             PARM       KWD(MRGOVR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Merged overrides')
             PARM       KWD(LVL) TYPE(*CHAR) LEN(10) DFT(*) +
                          SPCVAL((*) (*ACTGRPDFN) (*JOB)) +
                          PROMPT('Level')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
