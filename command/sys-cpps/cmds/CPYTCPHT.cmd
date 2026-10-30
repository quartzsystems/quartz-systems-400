/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYTCPHT: copy the host table to a file. */
             CMD        PROMPT('Copy TCP/IP Host Table') ALLOW(*ALL) +
                          HLPPNLGRP(QHTOCCMD) HLPID(CPYTCPHT)
             PARM       KWD(TOFILE) TYPE(Q1) PROMPT('To file')
             PARM       KWD(TOMBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('To member')
             PARM       KWD(MBROPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             PARM       KWD(TOOBJ) TYPE(*PNAME) LEN(5000) +
                          PROMPT('To stream file')
             PARM       KWD(REPLACE) TYPE(*INT4) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1)) +
                          PROMPT('Replace the stream file')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(QTCPHT)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
