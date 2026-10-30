/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPLNK: the links a path names. */
             CMD        PROMPT('Display Object Links') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(DSPLNK)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) DFT('*') +
                          EXPR(*YES) PROMPT('Object')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*) SPCVAL((* D) (*PRINT P)) PROMPT('Output')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALL) EXPR(*YES) SPCVAL(+
                          (*ALL ALL) (*ALLDIR ALLDIR) (*ALRTBL ALRTBL) +
                          (*AUTL AUTL) (*BLKSF BLKSF) (*BNDDIR BNDDIR) +
                          (*CFGL CFGL) (*CHTFMT CHTFMT) (*CHRSF CHRSF) +
                          (*CLD CLD) (*CLS CLS) (*CMD CMD) (*CNNL CNNL) +
                          (*COSD COSD) (*CRG CRG) (*CRQD CRQD) (*CSI CSI) +
                          (*CSPMAP CSPMAP) (*CSPTBL CSPTBL) (*CTLD CTLD) +
                          (*DDIR DDIR) (*DEVD DEVD) (*DIR DIR) (*DOC DOC) +
                          (*DSTMF DSTMF) (*DTAARA DTAARA) (*DTADCT DTADCT) +
                          (*DTAQ DTAQ) (*EDTD EDTD) (*EXITRG EXITRG) +
                          (*FCT FCT) (*FIFO FIFO) (*FILE FILE) (*FLR FLR) +
                          (*FNTRSC FNTRSC) (*FNTTBL FNTTBL) (*FORMDF FORMDF) +
                          (*FTR FTR) (*GSS GSS) (*IGCDCT IGCDCT) +
                          (*IGCSRT IGCSRT) (*IGCTBL IGCTBL) (*IMGCLG IMGCLG) +
                          (*IPXD IPXD) (*JOBD JOBD) (*JOBQ JOBQ) +
                          (*JOBSCD JOBSCD) (*JRN JRN) (*JRNRCV JRNRCV) +
                          (*LIB LIB) (*LIND LIND) (*LOCALE LOCALE) +
                          (*MBR MBR) (*MEDDFN MEDDFN) (*MENU MENU) +
                          (*MGTCOL MGTCOL) (*MODD MODD) (*MODULE MODULE) +
                          (*MSGF MSGF) (*MSGQ MSGQ) (*M36 M36) +
                          (*M36CFG M36CFG) (*NODGRP NODGRP) (*NODL NODL) +
                          (*NTBD NTBD) (*NWID NWID) (*NWSCFG NWSCFG) +
                          (*NWSD NWSD) (*OUTQ OUTQ) (*OVL OVL) +
                          (*PAGDFN PAGDFN) (*PAGSEG PAGSEG) (*PDFMAP PDFMAP) +
                          (*PDG PDG) (*PGM PGM) (*PNLGRP PNLGRP) +
                          (*PRDAVL PRDAVL) (*PRDDFN PRDDFN) (*PRDLOD PRDLOD) +
                          (*PSFCFG PSFCFG) (*QMFORM QMFORM) (*QMQRY QMQRY) +
                          (*QRYDFN QRYDFN) (*RCT RCT) (*SBSD SBSD) +
                          (*SCHIDX SCHIDX) (*SOCKET SOCKET) (*SPADCT SPADCT) +
                          (*SQLPKG SQLPKG) (*SQLUDT SQLUDT) (*SQLXSR SQLXSR) +
                          (*SRVPGM SRVPGM) (*SSND SSND) (*STMF STMF) +
                          (*SVRSTG SVRSTG) (*SYMLNK SYMLNK) (*S36 S36) +
                          (*TBL TBL) (*TIMZON TIMZON) (*USRIDX USRIDX) +
                          (*USRPRF USRPRF) (*USRQ USRQ) (*USRSPC USRSPC) +
                          (*VLDL VLDL) (*WSCST WSCST)) PROMPT('Object type')
             PARM       KWD(DETAIL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PRV) VALUES(*PRV *NAME *BASIC *EXTENDED) +
                          EXPR(*YES) PROMPT('Detail')
             PARM       KWD(DSPOPT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PRV) VALUES(*PRV *USER *ALL) +
                          PROMPT('Display option')
