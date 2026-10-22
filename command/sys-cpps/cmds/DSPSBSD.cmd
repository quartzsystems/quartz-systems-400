/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPSBSD: display a subsystem description and its entries. */
             CMD        PROMPT('Display Subsystem Description') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(DSPSBSD)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          DFT(*SELECT) VALUES(*SELECT *ALL *OPRATR *POOLS +
                          *AJE *WSNAME *WSTYPE *JOBQ *RTGE *CMNE +
                          *RMTLOCNAME *PJE) PROMPT('Option')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
