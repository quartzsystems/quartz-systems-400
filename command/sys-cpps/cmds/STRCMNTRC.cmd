/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRCMNTRC: start tracing a line's frames. */
             CMD        PROMPT('Start Communications Trace') +
                          ALLOW(*ALL) HLPPNLGRP(QHSSCMD) HLPID(STRCMNTRC)
             PARM       KWD(CFGOBJ) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Configuration object')
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*LIN *NWI *NWS) MIN(1) PROMPT('Type')
             PARM       KWD(MAXSTG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(128K) VALUES(128K 256K 512K 1M 2M 4M 8M +
                          16M 32M 64M 128M 256M 512M 1G *MAX) +
                          PROMPT('Buffer size')
             PARM       KWD(DTADIR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*BOTH) VALUES(*BOTH *SND *RCV) +
                          PROMPT('Data direction')
             PARM       KWD(TRCFULL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*WRAP) VALUES(*WRAP *STOPTRC) +
                          PROMPT('Trace full')
             PARM       KWD(USRDTA) TYPE(*INT4) DFT(*CALC) RANGE(1 +
                          65535) SPCVAL((*CALC 0) (*MAX -1)) +
                          PROMPT('Number of user bytes to trace')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Trace description')
