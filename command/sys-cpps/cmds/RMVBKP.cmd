/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVBKP: remove breakpoints from a program under debug. */
             CMD        PROMPT('Remove Breakpoint') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBGCMD) HLPID(RMVBKP)
             PARM       KWD(STMT) TYPE(*CHAR) LEN(10) MAX(10) DFT(*ALL) +
                          SNGVAL((*ALL)) PROMPT('Statement identifier')
             PARM       KWD(PGM) TYPE(Q1) DFT(*DFTPGM) SNGVAL((*DFTPGM) +
                          (*ALL)) PROMPT('Program')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
