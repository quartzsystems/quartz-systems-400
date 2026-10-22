/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGWTR: change the attributes of a started writer. */
             CMD        PROMPT('Change Writer') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(CHGWTR)
             PARM       KWD(WTR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Writer')
             PARM       KWD(FORMTYPE) TYPE(E1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Form type options')
             PARM       KWD(FILESEP) TYPE(*INT2) DFT(*SAME) RANGE(0 9) +
                          SPCVAL((*SAME -2) (*FILE -1)) +
                          PROMPT('File separators')
             PARM       KWD(SEPDRAWER) TYPE(*INT2) DFT(*SAME) +
                          RANGE(1 255) SPCVAL((*SAME -2) (*FILE -1) +
                          (*DEVD 0)) PROMPT('Drawer for separators')
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Output queue')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ANYRDYF) VALUES(*ANYRDYF *FILEEND) +
                          PROMPT('When to change writer')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*ALL) (*STD) (*FORMS)) +
                          PROMPT('Form type')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *INQMSG *MSG *NOMSG +
                          *INFOMSG) PROMPT('Message option')
