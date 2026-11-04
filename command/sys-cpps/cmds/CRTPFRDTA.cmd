/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTPFRDTA: the database files of a management collection. */
             CMD        PROMPT('Create Performance Data') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CRTPFRDTA)
             PARM       KWD(FROMMGTCOL) TYPE(Q1) MIN(1) +
                          PROMPT('From collection')
             PARM       KWD(TOMBR) TYPE(*NAME) LEN(10) DFT(*FROMMGTCOL) +
                          SPCVAL((*FROMMGTCOL *FROMMGT)) PROMPT('To member')
             PARM       KWD(TOLIB) TYPE(*NAME) LEN(10) DFT(*FROMMGTCOL) +
                          SPCVAL((*FROMMGTCOL *FROMMGT)) PROMPT('To library')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK)) +
                          PROMPT('Text ''description''')
             PARM       KWD(CGY) TYPE(*NAME) LEN(10) DFT(*FROMMGTCOL) +
                          SPCVAL((*FROMMGTCOL *FROMMGT) (*APPN) (*CMNBASE) +
                          (*DISK) (*JOBMI) (*JOBOS) (*POOL) (*SYSBUS) +
                          (*SYSLVL)) MAX(50) +
                          CHOICE('Name, *FROMMGTCOL, *APPN...') +
                          PROMPT('Categories to process')
             PARM       KWD(INTERVAL) TYPE(*CHAR) LEN(11) +
                          DFT(*FROMMGTCOL) SPCVAL((*FROMMGTCOL)) +
                          CHOICE('*FROMMGTCOL, 0.25, 0.5, 1...') +
                          PROMPT('Time interval (in minutes)')
             PARM       KWD(STRTIME) TYPE(E1) +
                          PROMPT('Starting date and time')
             PARM       KWD(ENDTIME) TYPE(E2) +
                          PROMPT('Ending date and time')
 Q1:         QUAL       TYPE(*NAME) LEN(10) SPCVAL((*ACTIVE) +
                          (*SELECT))
             QUAL       TYPE(*NAME) LEN(10) DFT(QPFRDATA) +
                          PROMPT('Library')
 E1:         ELEM       TYPE(*DATE) DFT(*FROMMGTCOL) +
                          SPCVAL((*FROMMGTCOL)) PROMPT('Starting date')
             ELEM       TYPE(*TIME) PROMPT('Starting time')
 E2:         ELEM       TYPE(*DATE) DFT(*FROMMGTCOL) +
                          SPCVAL((*FROMMGTCOL) (*ACTIVE)) +
                          PROMPT('Ending date')
             ELEM       TYPE(*TIME) PROMPT('Ending time')
