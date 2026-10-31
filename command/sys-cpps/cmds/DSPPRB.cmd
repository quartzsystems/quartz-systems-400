/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPPRB: show logged problems in detail, or print the problem log  */
/* report. The parameters are the pub400 prompt's; the selections    */
/* past PRBID and STATUS are accepted and do not narrow the list yet. */
             CMD        PROMPT('Display Problems') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(DSPPRB)
             PARM       KWD(PRBID) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Problem identifier')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALL) MAX(7) SNGVAL((*ALL)) VALUES(*OPENED +
                          *READY *PREPARED *SENT *ANSWERED *VERIFIED +
                          *CLOSED) PROMPT('Status type')
             PARM       KWD(SEV) TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*ALL) +
                          MAX(4) SNGVAL((*ALL)) VALUES(1 2 3 4) +
                          PROMPT('Severity')
             PARM       KWD(PERIOD) TYPE(E1) PROMPT('Period')
             PARM       KWD(HARDWARE) TYPE(E4) PROMPT('Hardware')
             PARM       KWD(RESOURCE) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Resource name')
             PARM       KWD(LICPGM) TYPE(E5) PROMPT('Product')
             PARM       KWD(FUNCTION) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Function')
             PARM       KWD(PGM) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Program')
             PARM       KWD(MSGID) TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Message identifier')
             PARM       KWD(ORIGIN) TYPE(E6) PROMPT('Origin')
             PARM       KWD(SRVID) TYPE(*CHAR) LEN(20) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Service number')
             PARM       KWD(BRANCH) TYPE(*CHAR) LEN(3) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Branch number')
             PARM       KWD(COUNTRY) TYPE(*CHAR) LEN(3) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Country or region number')
             PARM       KWD(ASNUSER) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User assigned')
             PARM       KWD(GROUP) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Group assigned')
             PARM       KWD(PRBTYPE) TYPE(*CHAR) LEN(4) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Problem type')
             PARM       KWD(PRBCGY) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Problem category')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             E1:        ELEM       TYPE(E2) PROMPT('Start time and date')
                        ELEM       TYPE(E3) PROMPT('End time and date')
             E2:        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('Start time')
                        ELEM       TYPE(*DATE) DFT(*BEGIN) +
                          SPCVAL((*BEGIN) (*CURRENT)) PROMPT('Start date')
             E3:        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('End time')
                        ELEM       TYPE(*DATE) DFT(*END) +
                          SPCVAL((*END) (*CURRENT)) PROMPT('End date')
             E4:        ELEM       TYPE(*CHAR) LEN(4) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Device type')
                        ELEM       TYPE(*CHAR) LEN(4) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Model number')
                        ELEM       TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Serial number')
             E5:        ELEM       TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Program identifier')
                        ELEM       TYPE(*CHAR) LEN(6) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Release')
                        ELEM       TYPE(*CHAR) LEN(4) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Modification level')
             E6:        ELEM       TYPE(*CHAR) LEN(8) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Network identifier')
                        ELEM       TYPE(*CHAR) LEN(8) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Control point name')
