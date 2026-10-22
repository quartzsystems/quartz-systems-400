/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKOUTQ: list output queues, or the spooled files of one. */
             CMD        PROMPT('Work with Output Queue') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(WRKOUTQ)
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*ALL) SNGVAL((*ALL)) +
                          PROMPT('Output queue')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*ALL) (*ALLUSR) +
                          (*USRLIBL)) PROMPT('Library')
