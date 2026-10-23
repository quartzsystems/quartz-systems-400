/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTELNA: the telnet server's attributes, kept in the data area     */
/* QUSRSYS/QTVTELNA and read when the server starts.                   */
             CMD        PROMPT('Change TELNET Attributes') +
                          ALLOW(*ALL) HLPPNLGRP(QTOCMD) HLPID(CHGTELNA)
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*YES) (*NO)) +
                          PROMPT('Autostart server')
             PARM       KWD(NBRSVR) TYPE(*DEC) LEN(3 0) DFT(*SAME) +
                          SPCVAL((*SAME 0) (*DFT 2)) RANGE(1 200) +
                          PROMPT('Number of server jobs')
             PARM       KWD(TIMMRKTIMO) TYPE(*DEC) LEN(10 0) DFT(*SAME) +
                          SPCVAL((*SAME 0) (*DFT 600)) RANGE(1 86400) +
                          PROMPT('Timemark timeout')
             PARM       KWD(INACTTIMO) TYPE(*DEC) LEN(10 0) DFT(*SAME) +
                          SPCVAL((*SAME 0) (*DFT 0)) RANGE(0 86400) +
                          PROMPT('Inactivity timeout')
             PARM       KWD(ALWSSL) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*YES) (*NO) (*ONLY)) +
                          PROMPT('Allow SSL')
