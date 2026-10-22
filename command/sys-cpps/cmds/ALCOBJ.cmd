/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ALCOBJ: lock objects for the job. */
             CMD        PROMPT('Allocate Object') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(ALCOBJ)
             PARM       KWD(OBJ) TYPE(E1) MIN(1) MAX(50) +
                          PROMPT('Object specifications')
             PARM       KWD(WAIT) TYPE(*DEC) LEN(5 0) DFT(*CLS) +
                          RANGE(0 32767) SPCVAL((*CLS -1)) +
                          PROMPT('Wait time (seconds)')
             PARM       KWD(SCOPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *LCKSPC *THREAD) +
                          PROMPT('Lock scope')
             PARM       KWD(CONFLICT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*RQSRLS) VALUES(*RQSRLS *NONE) +
                          PMTCTL(*PMTRQS) PROMPT('Lock conflict')
             E1:        ELEM       TYPE(Q1) MIN(1) PROMPT('Object')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
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
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          MIN(1) VALUES(*EXCL *EXCLRD *SHRUPD *SHRNUP +
                          *SHRRD) PROMPT('Lock state')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
