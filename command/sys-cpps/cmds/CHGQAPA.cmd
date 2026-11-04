/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGQAPA: the QAP server's attributes (docs/QAP.md), kept in the      */
/* TCP/IP server table in QUSRSYS.                                      */
             CMD        PROMPT('Change QAP Attributes') MAXPOS(1) +
                          HLPPNLGRP(QHTOCCMD) HLPID(CHGQAPA) +
                          ALLOW(*ALL)
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*YES *NO *SAME) +
                          PROMPT('Autostart server')
             PARM       KWD(PORT) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 65535) SPCVAL((*SAME -1) (*DFT 2320)) +
                          PROMPT('Port')
             PARM       KWD(IDLETIMO) TYPE(*INT4) DFT(*SAME) +
                          RANGE(60 86400) SPCVAL((*SAME -1) (*DFT +
                          3600)) PROMPT('Session idle timeout')
