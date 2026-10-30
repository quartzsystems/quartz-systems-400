/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* UNMOUNT: unmount a file system (RMVMFS is the same command). */
             CMD        PROMPT('Remove Mounted FS') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(UNMOUNT)
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) MIN(1) +
                          VALUES(*NFS *UDFS *ALL *NETWORK) +
                          PROMPT('Type of file system')
             PARM       KWD(MNTOVRDIR) TYPE(*PNAME) LEN(5000) +
                          DFT(*MFS) SPCVAL((*MFS)) +
                          PROMPT('Directory mounted over')
             PARM       KWD(MFS) TYPE(*PNAME) LEN(5000) +
                          PROMPT('Mounted file system')
