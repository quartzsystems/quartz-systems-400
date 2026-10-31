/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RSTUSRPRF: restore user profiles and authorization lists. */
             CMD        PROMPT('Restore User Profiles') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSRCMD) HLPID(RSTUSRPRF)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) MAX(4) +
                          SNGVAL((*SAVF) (*MEDDFN)) PROMPT('Device')
             PARM       KWD(USRPRF) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          MAX(300) SNGVAL((*ALL) (*NEW)) +
                          PROMPT('User profile')
             PARM       KWD(OMITUSRPRF) TYPE(*GENERIC) LEN(10) MAX(300) +
                          DFT(*NONE) SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('User profiles to omit')
             PARM       KWD(SECDTA) TYPE(*CHAR) LEN(8) DFT(*ALL) +
                          MAX(3) SNGVAL((*ALL)) RSTD(*YES) +
                          VALUES(*USRPRF *PWDGRP *PVTAUT) +
                          PMTCTL(*PMTRQS) PROMPT('Security data')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) MAX(75) +
                          DFT(*MOUNTED) SNGVAL((*MOUNTED)) +
                          PMTCTL(P1) PROMPT('Volume identifier')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(*SEARCH) +
                          RANGE(1 16777215) SPCVAL((*SEARCH 0)) +
                          PMTCTL(P1) PROMPT('Sequence number')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PMTCTL(P1) PROMPT('End of media option')
             PARM       KWD(SAVF) TYPE(Q1) PMTCTL(P2) +
                          PROMPT('Save file')
             PARM       KWD(MEDDFN) TYPE(Q1) PMTCTL(P3) +
                          PROMPT('Media definition')
             PARM       KWD(ALWOBJDIF) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) PMTCTL(*PMTRQS) +
                          PROMPT('Allow object differences')
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
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             P1:        PMTCTL     CTL(DEV) COND((*NE *SAVF) +
                          (*NE *MEDDFN)) NBRTRUE(*ALL)
             P2:        PMTCTL     CTL(DEV) COND((*EQ *SAVF))
             P3:        PMTCTL     CTL(DEV) COND((*EQ *MEDDFN))
