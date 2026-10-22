/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGOBJPGP: give an object a new primary group. */
             CMD        PROMPT('Change Object Primary Group') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGOBJPGP)
             PARM       KWD(OBJ) TYPE(Q1) MIN(1) PROMPT('Object')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MIN(1) VALUES(*ALRTBL *AUTL *BNDDIR *CFGL +
                          *CHTFMT *CLD *CLS *CMD *CNNL *COSD *CRQD +
                          *CSI *CSPMAP *CSPTBL *CTLD *DEVD *DTAARA +
                          *DTADCT *DTAQ *EDTD *EXITRG *FCT *FILE +
                          *FNTRSC *FNTTBL *FORMDF *FTR *GSS *IGCDCT +
                          *IGCSRT *IGCTBL *IMGCLG *IPXD *JOBD *JOBQ +
                          *JOBSCD *JRN *JRNRCV *LIB *LIND *LOCALE +
                          *MEDDFN *MENU *MGTCOL *MODD *MODULE *MSGF +
                          *MSGQ *M36 *M36CFG *NODGRP *NODL *NTBD +
                          *NWID *NWSCFG *NWSD *OUTQ *OVL *PAGDFN +
                          *PAGSEG *PDFMAP *PDG *PGM *PNLGRP *PRDAVL +
                          *PRDDFN *PRDLOD *PSFCFG *QMFORM *QMQRY +
                          *QRYDFN *RCT *SBSD *SCHIDX *SPADCT *SQLPKG +
                          *SQLUDT *SQLXSR *SRVPGM *SSND *S36 *SVRSTG +
                          *TBL *TIMZON *USRIDX *USRPRF *USRQ *USRSPC +
                          *VLDL *WSCST) PROMPT('Object type')
             PARM       KWD(NEWPGP) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*NONE)) PROMPT('New primary group')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*SYSBAS) (*CURASPGRP)) +
                          PROMPT('ASP device')
             PARM       KWD(PGPAUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*OLDPGP) VALUES(*OLDPGP *PRIVATE *ALL +
                          *CHANGE *USE *EXCLUDE) +
                          PROMPT('New primary group authority')
             PARM       KWD(RVKOLDAUT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Revoke old primary group')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
