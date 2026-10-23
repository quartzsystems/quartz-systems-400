/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRDFU: the Data File Utility's menu, or one of its options. */
             CMD        PROMPT('Start DFU') MAXPOS(4) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHDFUCMD) HLPID(STRDFU)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) DFT(*SELECT) +
                          RSTD(*YES) VALUES(*SELECT 1 2 3 4 5) +
                          PROMPT('Option')
             PARM       KWD(DFUPGM) TYPE(Q1) PROMPT('DFU program')
             PARM       KWD(FILE) TYPE(Q2) PROMPT('Data file')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
