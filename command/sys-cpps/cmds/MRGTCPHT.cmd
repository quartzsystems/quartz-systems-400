/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* MRGTCPHT: merge a host file into the host table. */
             CMD        PROMPT('Merge TCP/IP Host Table') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(MRGTCPHT)
             PARM       KWD(FROMFILE) TYPE(Q1) PROMPT('From file')
             PARM       KWD(FROMMBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*LAST)) +
                          PROMPT('From member')
             PARM       KWD(FROMOBJ) TYPE(*PNAME) LEN(5000) +
                          PROMPT('From stream file')
             PARM       KWD(FILEFMT) TYPE(*CHAR) LEN(32) RSTD(*YES) +
                          DFT(*OPSYS) SPCVAL((*OPSYS) (*AS400) (*AIX) +
                          (*NIC)) PROMPT('File format')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(32) RSTD(*YES) +
                          DFT(*NO) SPCVAL((*NO) (*YES)) +
                          PROMPT('Replace the table')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
