/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* MOVOBJ: move an object to another library. */
             CMD        PROMPT('Move Object') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(MOVOBJ)
             PARM       KWD(OBJ) TYPE(Q1) MIN(1) PROMPT('Object')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MIN(1) VALUES(*ALRTBL *BNDDIR *CHTFMT *CLD +
                          *CLS *CMD *CRQD *CSI *CSPMAP *CSPTBL *DTAARA +
                          *DTADCT *DTAQ *FCT *FILE *FNTRSC *FNTTBL +
                          *FORMDF *FTR *GSS *IGCDCT *IGCSRT *IGCTBL +
                          *IMGCLG *JOBD *JOBQ *JRN *JRNRCV *LOCALE +
                          *MEDDFN *MENU *MGTCOL *MODULE *MSGF *MSGQ +
                          *M36 *M36CFG *NODGRP *NODL *NWSCFG *OUTQ +
                          *OVL *PAGDFN *PAGSEG *PDFMAP *PDG *PGM +
                          *PNLGRP *PSFCFG *QMFORM *QMQRY *QRYDFN +
                          *SBSD *SCHIDX *SPADCT *SQLPKG *SQLUDT +
                          *SQLXSR *SRVPGM *SSND *S36 *SVRSTG *TBL +
                          *TIMZON *USRIDX *USRQ *USRSPC *VLDL *WSCST) +
                          PROMPT('Object type')
             PARM       KWD(TOLIB) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*CURLIB)) PROMPT('To library')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*CURASPGRP) (*SYSBAS)) +
                          PMTCTL(*PMTRQS) PROMPT('ASP device')
             PARM       KWD(TOASPDEV) TYPE(*NAME) LEN(10) DFT(*ASPDEV) +
                          SPCVAL((*ASPDEV) (*CURASPGRP) (*SYSBAS)) +
                          PMTCTL(*PMTRQS) PROMPT('To ASP device')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             DEP        CTL(*ALWAYS) PARM((&TOLIB *NE QTEMP)) MSGID(CPD0090)
