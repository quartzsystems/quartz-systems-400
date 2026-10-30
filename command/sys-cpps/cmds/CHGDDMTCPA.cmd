/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGDDMTCPA: the DDM server's TCP/IP attributes, kept in the data    */
/* area QUSRSYS/QRWDDMTCPA and read when the server starts.            */
             CMD        PROMPT('Change DDM TCP/IP Attributes') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHRWCMD) +
                          HLPID(CHGDDMTCPA)
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Autostart server')
             PARM       KWD(PWDRQD) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *USRID *VLDONLY +
                          *USRIDPWD *USRENCPWD *ENCUSRPWD *KERBEROS +
                          *NO *YES *ENCRYPTED) +
                          PROMPT('Lowest authentication method')
             PARM       KWD(ENCALG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *DES *AES) +
                          PROMPT('Lowest encryption algorithm')
