/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGMSGF: change a message file's text and CCSID. */
             CMD        PROMPT('Change Message File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(CHGMSGF)
             PARM       KWD(MSGF) TYPE(Q1) MIN(1) PROMPT('Message file')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 65535) SPCVAL((*SAME -1) (*HEX 65535) +
                          (*JOB 0) (*MSGD 65534)) +
                          PROMPT('Coded character set ID')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
