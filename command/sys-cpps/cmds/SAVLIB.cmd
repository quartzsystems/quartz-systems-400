/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SAVLIB: save libraries to a device or a save file. */
             CMD        PROMPT('Save Library') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSRCMD) HLPID(SAVLIB)
             PARM       KWD(LIB) TYPE(*GENERIC) LEN(10) MIN(1) MAX(300) +
                          SNGVAL((*NONSYS) (*ALLUSR) (*IBM)) +
                          PROMPT('Library')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) MAX(4) +
                          SNGVAL((*SAVF) (*MEDDFN)) PROMPT('Device')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) MAX(75) +
                          DFT(*MOUNTED) SNGVAL((*MOUNTED)) +
                          PMTCTL(P1) PROMPT('Volume identifier')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(*END) +
                          RANGE(1 16777215) SPCVAL((*END 0)) +
                          PMTCTL(P1) PROMPT('Sequence number')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*LIB) +
                          SPCVAL((*LIB)) PMTCTL(P1) PROMPT('File label')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*PERM) +
                          SPCVAL((*PERM)) PMTCTL(P1) +
                          PROMPT('File expiration date')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PMTCTL(P1) PROMPT('End of media option')
             PARM       KWD(STRLIB) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Starting library')
             PARM       KWD(SAVF) TYPE(Q1) PMTCTL(P2) +
                          PROMPT('Save file')
             PARM       KWD(MEDDFN) TYPE(Q1) PMTCTL(P3) +
                          PROMPT('Media definition')
             PARM       KWD(USEOPTBLK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(P1) +
                          PROMPT('Use optimum block size')
             PARM       KWD(SAVACT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *LIB *SYNCLIB *SYSDFN) +
                          PROMPT('Save active')
             PARM       KWD(SAVACTWAIT) TYPE(E1) +
                          PROMPT('Save active wait time')
             PARM       KWD(SAVACTMSGQ) TYPE(Q2) DFT(*NONE) +
                          SNGVAL((*NONE) (*WRKSTN)) +
                          PROMPT('Save active message queue')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*SYSBAS) (*CURASPGRP)) +
                          PROMPT('ASP device')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *PRINT *OUTFILE) +
                          PMTCTL(*PMTRQS) PROMPT('Output')
             PARM       KWD(OUTFILE) TYPE(Q1) PMTCTL(*PMTRQS) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E2) PMTCTL(*PMTRQS) +
                          PROMPT('Output member options')
             PARM       KWD(INFTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*OBJ) VALUES(*OBJ *LIB *MBR *ERR) +
                          PMTCTL(*PMTRQS) PROMPT('Type of output information')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PMTCTL(*PMTRQS) +
                          PROMPT('Target release')
             PARM       KWD(UPDHST) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Update history')
             PARM       KWD(OMITLIB) TYPE(*GENERIC) LEN(10) MAX(300) +
                          DFT(*NONE) SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Libraries to omit')
             PARM       KWD(OMITOBJ) TYPE(E3) MAX(300) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Objects to omit')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL *AFTER +
                          *REPLACE) PMTCTL(*PMTRQS) PROMPT('Clear')
             PARM       KWD(PRECHK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Object pre-check')
             PARM       KWD(SAVFDTA) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Save file data')
             PARM       KWD(SPLFDTA) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) PMTCTL(*PMTRQS) +
                          PROMPT('Spooled file data')
             PARM       KWD(QDTA) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *DTAQ) PMTCTL(*PMTRQS) +
                          PROMPT('Queue data')
             PARM       KWD(DTACPR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*DEV) VALUES(*DEV *NO *YES *LOW *MEDIUM +
                          *HIGH) PMTCTL(*PMTRQS) PROMPT('Data compression')
             PARM       KWD(COMPACT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*DEV) VALUES(*DEV *NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Data compaction')
             PARM       KWD(ACCPTH) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Save access paths')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT4) DFT(120) RANGE(0 99999) +
                          SPCVAL((*NOMAX -1)) PROMPT('Object locks')
                        ELEM       TYPE(*INT4) DFT(*LOCKWAIT) +
                          RANGE(0 99999) SPCVAL((*LOCKWAIT -2) +
                          (*NOCMTBDY -3) (*NOMAX -1)) +
                          PROMPT('Pending record changes')
                        ELEM       TYPE(*INT4) DFT(*LOCKWAIT) +
                          RANGE(0 99999) SPCVAL((*LOCKWAIT -2) +
                          (*NOMAX -1)) PROMPT('Other pending changes')
             E2:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             E3:        ELEM       TYPE(Q3) MIN(1) PROMPT('Object')
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
             P1:        PMTCTL     CTL(DEV) COND((*NE *SAVF) +
                          (*NE *MEDDFN)) NBRTRUE(*ALL)
             P2:        PMTCTL     CTL(DEV) COND((*EQ *SAVF))
             P3:        PMTCTL     CTL(DEV) COND((*EQ *MEDDFN))
