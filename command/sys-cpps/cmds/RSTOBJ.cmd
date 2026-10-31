/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RSTOBJ: restore objects saved from a library. */
             CMD        PROMPT('Restore Object') MAXPOS(3) ALLOW(*ALL) +
                          HLPPNLGRP(QHSRCMD) HLPID(RSTOBJ)
             PARM       KWD(OBJ) TYPE(*GENERIC) LEN(10) MIN(1) MAX(300) +
                          SPCVAL((*ALL)) PROMPT('Objects')
             PARM       KWD(SAVLIB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Saved library')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) MAX(4) +
                          SNGVAL((*SAVF) (*MEDDFN)) PROMPT('Device')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          MAX(72) SNGVAL((*ALL)) PROMPT('Object types')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) MAX(75) +
                          DFT(*MOUNTED) SNGVAL((*MOUNTED)) +
                          PMTCTL(P1) PROMPT('Volume identifier')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(*SEARCH) +
                          RANGE(1 16777215) SPCVAL((*SEARCH 0)) +
                          PMTCTL(P1) PROMPT('Sequence number')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*SAVLIB) +
                          SPCVAL((*SAVLIB)) PMTCTL(P1) +
                          PROMPT('File label')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PMTCTL(P1) PROMPT('End of media option')
             PARM       KWD(SAVF) TYPE(Q1) PMTCTL(P2) +
                          PROMPT('Save file')
             PARM       KWD(MEDDFN) TYPE(Q1) PMTCTL(P3) +
                          PROMPT('Media definition')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *NEW *OLD *FREE) +
                          PROMPT('Option')
             PARM       KWD(MBROPT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*MATCH) VALUES(*MATCH *ALL *NEW *OLD) +
                          PROMPT('Database member option')
             PARM       KWD(SPLFDTA) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NEW) VALUES(*NEW *NONE) PMTCTL(*PMTRQS) +
                          PROMPT('Spooled file data')
             PARM       KWD(ALWOBJDIF) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          MAX(4) DFT(*NONE) SNGVAL((*NONE) (*ALL) +
                          (*COMPATIBLE)) VALUES(*AUTL *FILELVL *OWNER +
                          *PGP) PROMPT('Allow object differences')
             PARM       KWD(RSTLIB) TYPE(*NAME) LEN(10) DFT(*SAVLIB) +
                          SPCVAL((*SAVLIB)) PROMPT('Restore to library')
             PARM       KWD(SAVASPDEV) TYPE(*NAME) LEN(10) DFT(*ANY) +
                          SPCVAL((*ANY) (*SYSBAS)) PMTCTL(*PMTRQS) +
                          PROMPT('Saved ASP device')
             PARM       KWD(RSTASPDEV) TYPE(*NAME) LEN(10) DFT(*SAVASPDEV) +
                          SPCVAL((*SAVASPDEV) (*SYSBAS) (*CURASPGRP)) +
                          PMTCTL(*PMTRQS) PROMPT('Restore to ASP device')
             PARM       KWD(FILEMBR) TYPE(E4) MAX(50) DFT(*ALL) +
                          SNGVAL((*ALL)) PMTCTL(*PMTRQS) +
                          PROMPT('File member')
             PARM       KWD(PVTAUT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Private authorities')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *PRINT *OUTFILE) +
                          PMTCTL(*PMTRQS) PROMPT('Output')
             PARM       KWD(OUTFILE) TYPE(Q1) PMTCTL(*PMTRQS) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E1) PMTCTL(*PMTRQS) +
                          PROMPT('Output member options')
             PARM       KWD(INFTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*OBJ) VALUES(*OBJ *MBR) PMTCTL(*PMTRQS) +
                          PROMPT('Type of output information')
             PARM       KWD(OMITOBJ) TYPE(E2) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Objects to omit')
             PARM       KWD(FRCOBJCVN) TYPE(E3) DFT(*SYSVAL) +
                          SNGVAL((*SYSVAL) (*NO)) PMTCTL(*PMTRQS) +
                          PROMPT('Force object conversion')
             PARM       KWD(DFRID) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*DFT)) PMTCTL(*PMTRQS) +
                          PROMPT('Defer ID')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             E2:        ELEM       TYPE(Q2) MIN(1) PROMPT('Object')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *ALRTBL *BNDDIR *CHTFMT +
                          *CLD *CLS *CMD *CRQD *CSI *CSPMAP *CSPTBL +
                          *DTAARA *DTADCT *DTAQ *FCT *FILE *FNTRSC +
                          *FNTTBL *FORMDF *FTR *GSS *IGCDCT *IGCSRT +
                          *IGCTBL *IMGCLG *JOBD *JOBQ *JRN *JRNRCV +
                          *LOCALE *MEDDFN *MENU *MGTCOL *MODULE *MSGF +
                          *MSGQ *M36 *M36CFG *NODGRP *NODL *NWSCFG +
                          *OUTQ *OVL *PAGDFN *PAGSEG *PDFMAP *PDG *PGM +
                          *PNLGRP *PSFCFG *QMFORM *QMQRY *QRYDFN *SBSD +
                          *SCHIDX *SPADCT *SQLPKG *SQLUDT *SQLXSR +
                          *SRVPGM *SSND *S36 *SVRSTG *TBL *TIMZON +
                          *USRIDX *USRQ *USRSPC *VLDL *WSCST) +
                          PROMPT('Object type')
             E3:        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES) PROMPT('Convert')
                        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*RQD) VALUES(*RQD *ALL) +
                          PROMPT('Objects to convert')
             E4:        ELEM       TYPE(*NAME) LEN(10) MIN(1) PROMPT('File')
                        ELEM       TYPE(*GENERIC) LEN(10) MAX(50) +
                          DFT(*ALL) SNGVAL((*ALL) (*NONE)) +
                          PROMPT('Member')
             P1:        PMTCTL     CTL(DEV) COND((*NE *SAVF) +
                          (*NE *MEDDFN)) NBRTRUE(*ALL)
             P2:        PMTCTL     CTL(DEV) COND((*EQ *SAVF))
             P3:        PMTCTL     CTL(DEV) COND((*EQ *MEDDFN))
