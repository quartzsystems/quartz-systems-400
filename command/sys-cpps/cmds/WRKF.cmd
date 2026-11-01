/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKF: the Work with Files display. */
             CMD        PROMPT('Work with Files') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(WRKF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(FILEATR) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *BSCF *CMNF *DDMF +
                          *DSPF *ICFF *LF *MXDF *PF *PRTF *SAVF +
                          *TAPF) PROMPT('File attributes')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) PROMPT('Library')
