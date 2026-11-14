/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGDIRSVRA: change a directory server instance's administrator, or */
/* the password of its update DN (docs/DIRSRV.md section 2.2).        */
             CMD        PROMPT('Change Directory Server Attr') +
                          MAXPOS(1) HLPPNLGRP(QHGLDCMD) +
                          HLPID(CHGDIRSVRA)
             PARM       KWD(INSTANCE) TYPE(*NAME) LEN(8) +
                          DFT(QUSRDIR) CHOICE('Name') +
                          PROMPT('Instance')
             PARM       KWD(DN) TYPE(E1) DFT(*SAME) SNGVAL((*SAME)) +
                          CHOICE('*SAME') PROMPT('Distinguished name')
 E1:         ELEM       TYPE(*CHAR) LEN(50) SPCVAL((*ADMINDN) +
                          (*UPDATEDN)) CHOICE('Character value') +
                          PROMPT('Distinguished name')
             ELEM       TYPE(*CHAR) LEN(50) +
                          CHOICE('Character value') PROMPT('Password')
