/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRRLU: a printer file's DDS edited as a page image. */
             CMD        PROMPT('Start Report Layout Utility') +
                          MAXPOS(5) ALLOW(*INTERACT *IPGM *IREXX *EXEC +
                          *IMOD) HLPPNLGRP(QHRLUCMD) HLPID(STRRLU)
             PARM       KWD(SRCFILE) TYPE(Q1) DFT(*PRV) SNGVAL((*PRV)) +
                          PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV)) PROMPT('Source member')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(1) DFT(2) +
                          RSTD(*YES) VALUES(2 5 6) PROMPT('Option')
             PARM       KWD(PAGWIDTH) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Page width')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) +
                          PROMPT('Text ''description''')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
