/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* GRTOBJAUT: grant users, the public or an authorization list authority */
/* to objects, or copy the authorities of a reference object.            */
             CMD        PROMPT('Grant Object Authority') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(GRTOBJAUT)
             PARM       KWD(OBJ) TYPE(Q1) MIN(1) PROMPT('Object')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MIN(1) VALUES(*ALL *ALRTBL *AUTL *BNDDIR *CFGL +
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
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*SYSBAS) (*CURASPGRP)) +
                          PROMPT('ASP device')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) MAX(50) +
                          SNGVAL((*PUBLIC)) PROMPT('Users')
             PARM       KWD(AUTL) TYPE(*NAME) LEN(10) +
                          PROMPT('Authorization list')
             PARM       KWD(AUT) TYPE(*CHAR) LEN(10) RSTD(*YES) MAX(10) +
                          DFT(*CHANGE) VALUES(*CHANGE *ALL *USE *EXCLUDE +
                          *AUTL *OBJALTER *OBJEXIST *OBJMGT *OBJOPR +
                          *OBJREF *ADD *DLT *EXECUTE *READ *UPD) +
                          PROMPT('Authority')
             PARM       KWD(REFOBJ) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('Reference object')
             PARM       KWD(REFOBJTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*OBJTYPE) VALUES(*OBJTYPE *ALRTBL *AUTL +
                          *BNDDIR *CFGL *CHTFMT *CLD *CLS *CMD *CNNL +
                          *COSD *CRQD *CSI *CSPMAP *CSPTBL *CTLD *DEVD +
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
                          *VLDL *WSCST) PROMPT('Reference object type')
             PARM       KWD(REFASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*SYSBAS) (*CURASPGRP)) +
                          PROMPT('Reference ASP device')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Replace authority')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
