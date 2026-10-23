/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHKOBJ: check that an object exists and that the user holds the */
/* authorities named.                                               */
             CMD        PROMPT('Check Object') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(CHKOBJ)
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
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*FIRST)) PROMPT('Member')
             PARM       KWD(AUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) SNGVAL((*NONE) (*ALL) (*CHANGE) (*USE) +
                          (*EXCLUDE)) VALUES(*AUTLMGT *OBJALTER *OBJEXIST *OBJMGT +
                          *OBJOPR *OBJREF *ADD *DLT *EXECUTE *READ *UPD) +
                          MAX(14) PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
