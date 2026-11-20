/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRQSIPAIR: make a one-time pairing password for a management        */
/* console and show it once (docs/QSID.md section 12). A security       */
/* officer's: *ALLOBJ and *SECADM. QS/400's: IBM has no QSI.            */
             CMD        PROMPT('Start QSI Pairing') ALLOW(*INTERACT *IPGM +
                          *IMOD *EXEC) HLPPNLGRP(QHTOCCMD) +
                          HLPID(STRQSIPAIR)
