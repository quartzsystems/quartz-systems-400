/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSPLF: list spooled files and work with them. */
             CMD        PROMPT('Work with Spooled Files') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(WRKSPLF)
             PARM       KWD(SELECT) TYPE(E1) PROMPT('Select files for')
             PARM       KWD(PERIOD) TYPE(E2) PROMPT('Time period')
             PARM       KWD(JOB) TYPE(QJ) DFT(*) PROMPT('Job name')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PRV) VALUES(*PRV *BASIC *INTERMED +
                          *ADVANCED) PROMPT('Assistance level')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*ALL)) PROMPT('User')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*OUTQ)) PROMPT('Print device')
                        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*STD)) PROMPT('Form type')
                        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User data')
                        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*ASPDEV) (*CURASPGRP) (*SYSBAS)) +
                          PROMPT('ASP')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALL) (*SYSBAS)) PROMPT('ASP device')
             E2:        ELEM       TYPE(E3) PROMPT('Start')
                        ELEM       TYPE(E4) PROMPT('End')
             E3:        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('Start time')
                        ELEM       TYPE(*DATE) DFT(*BEGIN) +
                          SPCVAL((*BEGIN) (*CURRENT)) PROMPT('Start date')
             E4:        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('End time')
                        ELEM       TYPE(*DATE) DFT(*END) +
                          SPCVAL((*END) (*CURRENT)) PROMPT('End date')
             QJ:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*))
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
