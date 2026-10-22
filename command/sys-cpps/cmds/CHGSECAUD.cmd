/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSECAUD: set the auditing system values and make sure QAUDJRN and */
/* its receiver exist.                                                   */
             CMD        PROMPT('Change Security Auditing') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGSECAUD)
             PARM       KWD(QAUDCTL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          MAX(3) DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          VALUES(*AUDLVL *OBJAUD *NOQTEMP) +
                          PROMPT('Auditing control')
             PARM       KWD(QAUDLVL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MAX(32) DFT(*SAME) SNGVAL((*SAME) (*NONE) +
                          (*DFTSET)) VALUES(*ATNEVT *AUTFAIL *CREATE +
                          *DELETE *JOBBAS *JOBCHGUSR *JOBDTA *NETBAS +
                          *NETCLU *NETCMN *NETFAIL *NETSCK *NETSECURE +
                          *NETTELSVR *NETUDP *OBJMGT *OFCSRV *OPTICAL +
                          *PGMADP *PGMFAIL *PRTDTA *PTFOBJ *PTFOPR +
                          *SAVRST *SECCFG *SECDIRSRV *SECIPC *SECNAS +
                          *SECRUN *SECSCKD *SECURITY *SECVFY *SECVLDL +
                          *SERVICE *SPLFDTA *SYSMGT *AUDLVL2) +
                          PROMPT('Auditing level')
             PARM       KWD(QAUDLVL2) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MAX(32) DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          VALUES(*ATNEVT *AUTFAIL *CREATE *DELETE +
                          *JOBBAS *JOBCHGUSR *JOBDTA *NETBAS *NETCLU +
                          *NETCMN *NETFAIL *NETSCK *NETSECURE +
                          *NETTELSVR *NETUDP *OBJMGT *OFCSRV *OPTICAL +
                          *PGMADP *PGMFAIL *PRTDTA *PTFOBJ *PTFOPR +
                          *SAVRST *SECCFG *SECDIRSRV *SECIPC *SECNAS +
                          *SECRUN *SECSCKD *SECURITY *SECVFY *SECVLDL +
                          *SERVICE *SPLFDTA *SYSMGT) +
                          PROMPT('Auditing level extension')
             PARM       KWD(JRNRCV) TYPE(Q1) DFT(*GEN) SNGVAL((*GEN)) +
                          PROMPT('Initial journal receiver')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
