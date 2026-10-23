/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPOBJD: show object descriptions, on the display, printed or in */
/* an output file.                                                   */
             CMD        PROMPT('Display Object Description') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(DSPOBJD)
             PARM       KWD(OBJ) TYPE(Q1) MIN(1) PROMPT('Object')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MIN(1) MAX(110) SNGVAL((*ALL)) +
                          VALUES(*ALRTBL *AUTL *BNDDIR *CFGL +
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
             PARM       KWD(DETAIL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*BASIC) VALUES(*BASIC *FULL *SERVICE) +
                          PROMPT('Detail')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALLAVL) (*CURASPGRP) (*SYSBAS)) +
                          PMTCTL(*PMTRQS) PROMPT('ASP device')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT *OUTFILE) +
                          PROMPT('Output')
             PARM       KWD(OUTFILE) TYPE(Q2) PMTCTL(P1) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E1) PMTCTL(P1) +
                          PROMPT('Output member options')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR) (*ALLAVL) (*ALLUSRAVL)) +
                          PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             P1:        PMTCTL     CTL(OUTPUT) COND((*EQ *OUTFILE))
             DEP        CTL(&OUTFILE) PARM((&OUTPUT *EQ *OUTFILE)) +
                          MSGID(CPD9862)
