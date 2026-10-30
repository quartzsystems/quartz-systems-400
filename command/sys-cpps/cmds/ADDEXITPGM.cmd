/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDEXITPGM: register a program on an exit point and format. */
             CMD        PROMPT('Add Exit Program') MAXPOS(4) +
                          ALLOW(*ALL) HLPPNLGRP(QHUSCMD) HLPID(ADDEXITPGM)
             PARM       KWD(EXITPNT) TYPE(*CHAR) LEN(20) MIN(1) +
                          PROMPT('Exit point')
             PARM       KWD(FORMAT) TYPE(*NAME) LEN(8) MIN(1) +
                          PROMPT('Exit point format')
             PARM       KWD(PGMNBR) TYPE(*INT4) MIN(1) +
                          RANGE(1 2147483647) SPCVAL((*LOW -1) (*HIGH +
                          -2)) PROMPT('Program number')
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PROMPT('Program')
             PARM       KWD(THDSAFE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*UNKNOWN) VALUES(*UNKNOWN *NO *YES) +
                          PROMPT('Threadsafe')
             PARM       KWD(MLTTHDACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *RUN *MSG *NORUN) +
                          PROMPT('Multithreaded job action')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text ''description''')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Replace an existing program')
             PARM       KWD(CRTEXITPNT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Create the exit point')
             PARM       KWD(PGMDTA) TYPE(E1) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('Exit program data')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT4) DFT(*JOB) RANGE(1 65535) +
                          SPCVAL((*JOB 0) (*HEX 65535)) +
                          PROMPT('Coded character set ID')
                        ELEM       TYPE(*INT4) DFT(*CALC) RANGE(0 2048) +
                          SPCVAL((*CALC -1)) PROMPT('Length of data')
                        ELEM       TYPE(*CHAR) LEN(2048) PROMPT('Program data')
