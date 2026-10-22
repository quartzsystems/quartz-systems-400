/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGUSRAUD: set the object and action auditing of user profiles. */
             CMD        PROMPT('Change User Auditing') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGUSRAUD)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) MIN(1) MAX(50) +
                          PROMPT('User profile')
             PARM       KWD(OBJAUD) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *CHANGE *ALL) +
                          PROMPT('Object auditing value')
             PARM       KWD(AUDLVL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MAX(32) DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          VALUES(*ATNEVT *CMD *CREATE *DELETE *JOBBAS +
                          *JOBCHGUSR *JOBDTA *NETBAS *NETCLU *NETCMN +
                          *NETFAIL *NETSCK *NETSECURE *NETTELSVR +
                          *NETUDP *OBJMGT *OFCSRV *OPTICAL *PGMADP +
                          *PGMFAIL *PRTDTA *PTFOBJ *PTFOPR *SAVRST +
                          *SECCFG *SECDIRSRV *SECIPC *SECNAS *SECRUN +
                          *SECSCKD *SECURITY *SECVFY *SECVLDL *SERVICE +
                          *SPLFDTA *SYSMGT) PROMPT('User action auditing')
