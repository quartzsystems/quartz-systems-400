/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKOBJOWN: list the objects a user owns and work with them. */
             CMD        PROMPT('Work with Objects by Owner') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(WRKOBJOWN)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT)) PROMPT('User profile')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *ALRTBL *AUTL *BNDDIR +
                          *CFGL *CHTFMT *CLD *CLS *CMD *CNNL *COSD +
                          *CRQD *CSI *CSPMAP *CSPTBL *CTLD *DEVD +
                          *DTAARA *DTADCT *DTAQ *EDTD *EXITRG *FCT +
                          *FILE *FNTRSC *FNTTBL *FORMDF *FTR *GSS +
                          *IGCDCT *IGCSRT *IGCTBL *IMGCLG *IPXD *JOBD +
                          *JOBQ *JOBSCD *JRN *JRNRCV *LIB *LIND +
                          *LOCALE *MEDDFN *MENU *MGTCOL *MODD *MODULE +
                          *MSGF *MSGQ *M36 *M36CFG *NODGRP *NODL +
                          *NTBD *NWID *NWSCFG *NWSD *OUTQ *OVL *PAGDFN +
                          *PAGSEG *PDFMAP *PDG *PGM *PNLGRP *PRDAVL +
                          *PRDDFN *PRDLOD *PSFCFG *QMFORM *QMQRY +
                          *QRYDFN *RCT *SBSD *SCHIDX *SPADCT *SQLPKG +
                          *SQLUDT *SQLXSR *SRVPGM *SSND *S36 *SVRSTG +
                          *TBL *TIMZON *USRIDX *USRPRF *USRQ *USRSPC +
                          *VLDL *WSCST) PROMPT('Object type')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
