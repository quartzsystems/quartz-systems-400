/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVMFS: unmount a file system (UNMOUNT is the same command).         */
             CMD        PROMPT('Remove Mounted FS') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(RMVMFS)
             PARM       KWD(TYPE) TYPE(*INT4) RSTD(*YES) MIN(1) +
                          SPCVAL((*NFS -1) (*UDFS -2) (*ALL -4)) +
                          PROMPT('Type of file system')
             PARM       KWD(MNTOVRDIR) TYPE(*PNAME) LEN(5000) +
                          SPCVAL((*ALL)) CHOICE('Path name') +
                          PROMPT('Directory mounted over')
             PARM       KWD(MFS) TYPE(*PNAME) LEN(5000) PMTCTL(PUDFS) +
                          CHOICE('Path name') +
                          PROMPT('Mounted file system')
 PUDFS:      PMTCTL     CTL(TYPE) COND((*EQ *UDFS)) NBRTRUE(*GT 0)
             DEP        CTL((&TYPE *EQ -2)) PARM((&MFS *NE ' ') +
                          (&MNTOVRDIR *NE ' ')) NBRTRUE(*NE 0) +
                          MSGID(CPDA1B2)
             DEP        CTL((&TYPE *EQ -2)) PARM((&MFS *NE ' ') +
                          (&MNTOVRDIR *NE ' ')) NBRTRUE(*NE 2) +
                          MSGID(CPDA1B3)
             DEP        CTL((&TYPE *NE -2)) PARM((&MFS *NE ' ')) +
                          NBRTRUE(*EQ 0) MSGID(CPDA1B4)
             DEP        CTL((&TYPE *NE -2)) PARM((&MNTOVRDIR *NE ' ')) +
                          NBRTRUE(*EQ 1) MSGID(CPDA1B5)
             DEP        CTL((&MNTOVRDIR *EQ *ALL)) PARM((&TYPE *EQ -4)) +
                          NBRTRUE(*EQ 1) MSGID(CPDA1C7)
