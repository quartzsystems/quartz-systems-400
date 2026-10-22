/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKJOBD: work with job descriptions. */
             CMD        PROMPT('Work with Job Descriptions') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKJOBD)
             PARM       KWD(JOBD) TYPE(Q1) DFT(*ALL) PROMPT('Job description')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) PROMPT('Library')
