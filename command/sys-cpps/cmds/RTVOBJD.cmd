/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVOBJD: put the description of an object into CL variables. */
             CMD        PROMPT('Retrieve Object Description') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RTVOBJD)
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
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALLAVL) (*CURASPGRP) (*SYSBAS)) +
                          PROMPT('ASP device')
             PARM       KWD(RTNLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for RTNLIB (10)')
             PARM       KWD(OBJATR) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for OBJATR (10)')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) RTNVAL(*YES) +
                          PROMPT('CL var for TEXT (50)')
             PARM       KWD(OWNER) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for OWNER (10)')
             PARM       KWD(PGP) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for PGP (10)')
             PARM       KWD(ASP) TYPE(*DEC) LEN(2 0) RTNVAL(*YES) +
                          PROMPT('CL var for ASP (2 0)')
             PARM       KWD(OVFASP) TYPE(*CHAR) LEN(1) RTNVAL(*YES) +
                          PROMPT('CL var for OVFASP (1)')
             PARM       KWD(CRTDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for CRTDATE (13)')
             PARM       KWD(CHGDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for CHGDATE (13)')
             PARM       KWD(SAVDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for SAVDATE (13)')
             PARM       KWD(RSTDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for RSTDATE (13)')
             PARM       KWD(CRTUSER) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for CRTUSER (10)')
             PARM       KWD(CRTSYSNAME) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('CL var for CRTSYSNAME (8)')
             PARM       KWD(SYSLVL) TYPE(*CHAR) LEN(9) RTNVAL(*YES) +
                          PROMPT('CL var for SYSLVL (9)')
             PARM       KWD(COMPILER) TYPE(*CHAR) LEN(16) RTNVAL(*YES) +
                          PROMPT('CL var for COMPILER (16)')
             PARM       KWD(OBJLVL) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('CL var for OBJLVL (8)')
             PARM       KWD(USRCHG) TYPE(*CHAR) LEN(1) RTNVAL(*YES) +
                          PROMPT('CL var for USRCHG (1)')
             PARM       KWD(USRDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for USRDATE (13)')
             PARM       KWD(USRDATA) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for USRDATA (10)')
             PARM       KWD(SIZE) TYPE(*DEC) LEN(15 0) RTNVAL(*YES) +
                          PROMPT('CL var for SIZE (15 0)')
             PARM       KWD(SAVSIZE) TYPE(*DEC) LEN(15 0) RTNVAL(*YES) +
                          PROMPT('CL var for SAVSIZE (15 0)')
             PARM       KWD(SAVSEQNBR) TYPE(*DEC) LEN(4 0) RTNVAL(*YES) +
                          PROMPT('CL var for SAVSEQNBR (4 0)')
             PARM       KWD(SAVCMD) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SAVCMD (10)')
             PARM       KWD(SAVVOL) TYPE(*CHAR) LEN(71) RTNVAL(*YES) +
                          PROMPT('CL var for SAVVOL (71)')
             PARM       KWD(SAVDEV) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SAVDEV (10)')
             PARM       KWD(SAVFILE) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SAVFILE (10)')
             PARM       KWD(SAVFILELIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SAVFILELIB (10)')
             PARM       KWD(SAVLABEL) TYPE(*CHAR) LEN(17) RTNVAL(*YES) +
                          PROMPT('CL var for SAVLABEL (17)')
             PARM       KWD(SRCFILE) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SRCFILE (10)')
             PARM       KWD(SRCFILELIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SRCFILELIB (10)')
             PARM       KWD(SRCMBR) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SRCMBR (10)')
             PARM       KWD(SRCUPDDATE) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for SRCUPDDATE (13)')
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for LICPGM (7)')
             PARM       KWD(PTF) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for PTF (7)')
             PARM       KWD(APAR) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for APAR (10)')
             PARM       KWD(OBJDMN) TYPE(*CHAR) LEN(2) RTNVAL(*YES) +
                          PROMPT('CL var for OBJDMN (2)')
             PARM       KWD(OBJAUDVAL) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for OBJAUDVAL (10)')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
