/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGPFM: change a physical file member's attributes. */
             CMD        PROMPT('Change Physical File Member') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CHGPFM)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Member')
             PARM       KWD(SRCTYPE) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PROMPT('Source type')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) +
                          PROMPT('Expiration date for member')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Share open data path')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
