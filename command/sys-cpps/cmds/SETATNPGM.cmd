/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SETATNPGM: set the program the Attention key runs for the job. */
             CMD        PROMPT('Set Attention Program') +
                          ALLOW(*INTERACT *IPGM *IMOD *IREXX *EXEC) +
                          HLPPNLGRP(QHWCCMD) HLPID(SETATNPGM)
             PARM       KWD(PGM) TYPE(Q1) MIN(1) +
                          SNGVAL((*CURRENT) (*PRV)) +
                          PROMPT('Program')
             PARM       KWD(SET) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*ON) VALUES(*ON *OFF) +
                          PROMPT('Set attention key')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
