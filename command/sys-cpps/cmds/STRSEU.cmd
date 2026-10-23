/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRSEU: edit, browse or print a source member with SEU. */
             CMD        PROMPT('Start Source Entry Utility') +
                          MAXPOS(5) ALLOW(*INTERACT *IPGM *IMOD *IREXX +
                          *EXEC) HLPPNLGRP(QHSUCMD) HLPID(STRSEU)
             PARM       KWD(SRCFILE) TYPE(Q1) DFT(*PRV) SNGVAL((*PRV)) +
                          PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*SELECT)) PROMPT('Source member')
             PARM       KWD(TYPE) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Source type')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*BLANK) VALUES('2' '5' '6') +
                          SPCVAL((*BLANK ' ')) PROMPT('Option')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) CASE(*MIXED) +
                          PROMPT('Text ''description''')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
