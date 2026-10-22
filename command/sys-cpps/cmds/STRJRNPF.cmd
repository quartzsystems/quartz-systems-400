/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRJRNPF: start journaling changes to physical files. */
             CMD        PROMPT('Start Journal Physical File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(STRJRNPF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) MAX(50) +
                          PROMPT('Physical file to be journaled')
             PARM       KWD(JRN) TYPE(Q1) MIN(1) PROMPT('Journal')
             PARM       KWD(IMAGES) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*AFTER) VALUES(*AFTER *BOTH) +
                          PROMPT('Record images')
             PARM       KWD(OMTJRNE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *OPNCLO) +
                          PROMPT('Journal entries to be omitted')
             PARM       KWD(LOGLVL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ERRORS) VALUES(*ERRORS *ALL) +
                          PROMPT('Logging level')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
