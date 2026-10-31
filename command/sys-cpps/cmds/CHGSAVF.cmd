/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSAVF: change a save file's attributes. */
             CMD        PROMPT('Change Save File') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSRCMD) HLPID(CHGSAVF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('Save file')
             PARM       KWD(MAXRCDS) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 2147483647) SPCVAL((*SAME -1) +
                          (*NOMAX 0)) PROMPT('Maximum records')
             PARM       KWD(WAITFILE) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -2) (*IMMED 0) +
                          (*CLS -1)) PROMPT('Maximum file wait time')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Share open data path')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK '')) +
                          PROMPT('Text ''description''')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
