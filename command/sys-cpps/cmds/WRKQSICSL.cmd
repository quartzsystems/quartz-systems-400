/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKQSICSL: the management consoles paired with the system and its    */
/* QSI fingerprint (docs/QSID.md section 12); option 4 removes one.     */
/* A security officer's: *ALLOBJ and *SECADM. QS/400's.                 */
             CMD        PROMPT('Work with QSI Consoles') ALLOW(*ALL) +
                          HLPPNLGRP(QHTOCCMD) HLPID(WRKQSICSL)
