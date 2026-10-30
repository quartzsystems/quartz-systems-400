/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGLPDA: the line printer daemon's attributes, kept in the TCP/IP   */
/* server table in QUSRSYS. Keywords and ranges are the oracle's.       */
             CMD        PROMPT('Change LPD Attributes') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTMCMD) HLPID(CHGLPDA)
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*YES *NO *SAME) +
                          PROMPT('Autostart servers')
             PARM       KWD(NBRSVR) TYPE(*INT2) DFT(*SAME) +
                          RANGE(1 200) SPCVAL((*SAME -1) (*DFT 2)) +
                          PROMPT('Number of initial servers')
