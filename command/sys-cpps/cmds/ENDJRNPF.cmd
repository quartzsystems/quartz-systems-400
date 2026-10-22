/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDJRNPF: end journaling of physical files. */
             CMD        PROMPT('End Journal Physical File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(ENDJRNPF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) MAX(50) SNGVAL((*ALL)) +
                          PROMPT('Physical file')
             PARM       KWD(JRN) TYPE(Q2) PROMPT('Journal')
             PARM       KWD(LOGLVL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ERRORS) VALUES(*ERRORS *ALL) +
                          PROMPT('Logging level')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             DEP        CTL(&FILE *EQ *ALL) PARM(&JRN) NBRTRUE(*ALL) +
                          MSGID(CPD7008)
