/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGQSIA: the QSI server's attributes (docs/QSID.md section 12), kept */
/* in the TCP/IP server table in QUSRSYS. QS/400's: IBM has no QSI.     */
             CMD        PROMPT('Change QSI Attributes') MAXPOS(1) +
                          HLPPNLGRP(QHTOCCMD) HLPID(CHGQSIA) +
                          ALLOW(*ALL)
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*YES *NO *SAME) +
                          PROMPT('Autostart server')
             PARM       KWD(PORT) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 65535) SPCVAL((*SAME -1) (*DFT 2300)) +
                          PROMPT('Port')
