/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTPUBAUT: print the objects of one type the public can use. */
             CMD        PROMPT('Print Public Authorities') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(PRTPUBAUT)
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MIN(1) VALUES(*ALRTBL *AUTL *BNDDIR *CFGL +
                          *CHTFMT *CLD *CLS *CMD *CNNL *COSD *CRQD +
                          *CSI *CSPMAP *CSPTBL *CTLD *DEVD *DIR *DOC +
                          *DTAARA *DTADCT *DTAQ *EDTD *EXITRG *FCT +
                          *FILE *FLR *FNTRSC *FNTTBL *FORMDF *FTR *GSS +
                          *IGCDCT *IGCSRT *IGCTBL *IMGCLG *IPXD *JOBD +
                          *JOBQ *JOBSCD *JRN *JRNRCV *LIB *LIND +
                          *LOCALE *MEDDFN *MENU *MGTCOL *MODD *MODULE +
                          *MSGF *MSGQ *M36 *M36CFG *NODGRP *NODL +
                          *NTBD *NWID *NWSCFG *NWSD *OUTQ *OVL *PAGDFN +
                          *PAGSEG *PDFMAP *PDG *PGM *PNLGRP *PRDAVL +
                          *PRDDFN *PRDLOD *PSFCFG *QMFORM *QMQRY +
                          *QRYDFN *RCT *SBSD *SCHIDX *SPADCT *SQLPKG +
                          *SQLUDT *SQLXSR *SRVPGM *SSND *STMF *SYMLNK +
                          *S36 *SVRSTG *TBL *TIMZON *USRIDX *USRPRF +
                          *USRQ *USRSPC *VLDL *WSCST) +
                          PROMPT('Object type')
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*ALLUSR) (*LIBL) (*CURLIB)) +
                          PROMPT('Library')
             PARM       KWD(FOLDER) TYPE(*CHAR) LEN(63) DFT(*ANY) +
                          SPCVAL((*ANY) (*NONE)) CASE(*MIXED) +
                          PROMPT('Folder')
