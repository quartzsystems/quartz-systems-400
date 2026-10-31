/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CLRSAVF: empty a save file. */
             CMD        PROMPT('Clear Save File') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSRCMD) HLPID(CLRSAVF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('Save file')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
