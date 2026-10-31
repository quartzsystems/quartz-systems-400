/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGLICINF: change a product's license information. */
             CMD        PROMPT('Change License Information') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHLZCMD) +
                          HLPID(CHGLICINF)
             PARM       KWD(PRDID) TYPE(*CHAR) LEN(7) MIN(1) +
                          PROMPT('Product identifier')
             PARM       KWD(LICTRM) TYPE(*CHAR) LEN(6) MIN(1) +
                          PROMPT('License term')
             PARM       KWD(FEATURE) TYPE(*CHAR) LEN(4) MIN(1) +
                          PROMPT('Feature')
             PARM       KWD(USGLMT) TYPE(*CHAR) LEN(7) DFT(*SAME) +
                          SPCVAL((*SAME) (*NOMAX)) PROMPT('Usage limit')
             PARM       KWD(THRESHOLD) TYPE(*CHAR) LEN(7) DFT(*SAME) +
                          SPCVAL((*SAME) (*USGLMT) (*CALC)) +
                          PROMPT('Threshold')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Message queue')
             PARM       KWD(LOG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Log')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
