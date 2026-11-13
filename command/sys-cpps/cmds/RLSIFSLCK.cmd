/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RLSIFSLCK: release the byte-range locks a remote host holds through */
/* the NFS server, or those held on an object or its file system.      */
             CMD        PROMPT('Release File System Locks') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) +
                          HLPID(RLSIFSLCK)
             PARM       KWD(RMTLOCNAME) TYPE(*CHAR) LEN(256) +
                          CHOICE('Character value') +
                          PROMPT('Remote location')
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) +
                          CHOICE('Path name') PROMPT('Object')
             PARM       KWD(NFSSVRLCK) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*YES) SPCVAL((*NO N) (*YES Y)) +
                          PMTCTL(POBJ) PROMPT('NFS Server locks')
             PARM       KWD(LCLBRNGLCK) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*YES) SPCVAL((*NO N) (*YES Y)) +
                          PMTCTL(POBJ) PROMPT('Local byte-range locks')
             PARM       KWD(OBJMFS) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*NO) SPCVAL((*NO N) (*YES Y)) +
                          PMTCTL(POBJ) PROMPT('Object''s file system')
 POBJ:       PMTCTL     CTL(OBJ) COND((*NE ' ')) NBRTRUE(*EQ 1)
             DEP        CTL(*ALWAYS) PARM((&RMTLOCNAME *NE ' ') +
                          (&OBJ *NE ' ')) NBRTRUE(*NE 0) MSGID(CPDA1B6)
             DEP        CTL(*ALWAYS) PARM((&RMTLOCNAME *NE ' ') +
                          (&OBJ *NE ' ')) NBRTRUE(*NE 2) MSGID(CPDA1B7)
             DEP        CTL(*ALWAYS) PARM((&LCLBRNGLCK *EQ Y) +
                          (&OBJMFS *EQ Y)) NBRTRUE(*NE 2) MSGID(CPDA1D8)
