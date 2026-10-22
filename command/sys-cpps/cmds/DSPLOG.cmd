/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPLOG: show the history log over a period, by job or message. */
             CMD        PROMPT('Display Log') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(DSPLOG)
             PARM       KWD(LOG) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(QHST) VALUES(QHST) PROMPT('Log')
             PARM       KWD(PERIOD) TYPE(E1) PROMPT('Time period')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT *PRTWRAP *PRTSECLVL) +
                          PROMPT('Output')
             PARM       KWD(JOB) TYPE(Q1) MAX(5) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Jobs')
             PARM       KWD(MSGID) TYPE(*GENERIC) LEN(7) MAX(100) +
                          DFT(*ALL) SNGVAL((*ALL)) PMTCTL(*PMTRQS) +
                          PROMPT('Message identifiers')
             E1:        ELEM       TYPE(E2) PROMPT('Start')
                        ELEM       TYPE(E3) PROMPT('End')
             E2:        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('Time')
                        ELEM       TYPE(*DATE) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*BEGIN)) PROMPT('Date')
             E3:        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('Time')
                        ELEM       TYPE(*DATE) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*END)) PROMPT('Date')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Job name')
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) PROMPT('Number')
