/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTCPSVR: change a user-defined server. */
             CMD        PROMPT('Change TCP/IP Server') MAXPOS(6) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(CHGTCPSVR)
             PARM       KWD(SVRSPCVAL) TYPE(*CHAR) LEN(10) MIN(1) +
                          PROMPT('Server special value')
             PARM       KWD(PGM) TYPE(Q1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Server program')
             PARM       KWD(SVRNAME) TYPE(*CHAR) LEN(30) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Server name')
             PARM       KWD(SVRTYPE) TYPE(*CHAR) LEN(30) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Server type')
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME S) (*YES Y) (*NO N)) +
                          PROMPT('Start with TCP/IP')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PROMPT('Text')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library')
