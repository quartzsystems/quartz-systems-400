/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* OVRDBF: override a database file's name and open attributes for */
/* the programs that run after it.                                   */
             CMD        PROMPT('Override With Database File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(OVRDBF)
             PARM       KWD(FILE) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('File being overridden')
             PARM       KWD(TOFILE) TYPE(Q1) DFT(*FILE) SNGVAL((*FILE)) +
                          PROMPT('Overriding to database file')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) +
                          SPCVAL((*FIRST) (*LAST) (*ALL)) PROMPT('Member')
             PARM       KWD(POSITION) TYPE(E1) SNGVAL((*NONE) (*START) +
                          (*END)) PROMPT('Starting position in file')
             PARM       KWD(RCDFMTLCK) TYPE(E2) MAX(50) +
                          PROMPT('Record format lock')
             PARM       KWD(FRCRATIO) TYPE(*DEC) LEN(5) RANGE(1 32767) +
                          SPCVAL((*NONE 0)) PROMPT('Records to force a write')
             PARM       KWD(FMTSLR) TYPE(Q2) SNGVAL((*NONE)) +
                          PROMPT('Format selector program')
             PARM       KWD(WAITFILE) TYPE(*DEC) LEN(5) RANGE(1 32767) +
                          SPCVAL((*IMMED 0) (*CLS -1)) +
                          PROMPT('Maximum file wait time')
             PARM       KWD(WAITRCD) TYPE(*DEC) LEN(5) RANGE(1 32767) +
                          SPCVAL((*IMMED 0) (*NOMAX -1)) +
                          PROMPT('Maximum record wait time')
             PARM       KWD(SEQONLY) TYPE(E3) PROMPT('Limit to sequential only')
             PARM       KWD(NBRRCDS) TYPE(*DEC) LEN(5) RANGE(1 32767) +
                          PROMPT('Records retrieved at once')
             PARM       KWD(EOFDLY) TYPE(*DEC) LEN(5) RANGE(1 99999) +
                          SPCVAL((*NONE 0)) PROMPT('End of file delay')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*YES *NO) PROMPT('Record format level check')
             PARM       KWD(EXPCHK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Expiration date check')
             PARM       KWD(INHWRT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Inhibit write')
             PARM       KWD(SECURE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Secure from other overrides')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*NO *YES) PROMPT('Share open data path')
             PARM       KWD(OPNSCOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ACTGRPDFN) VALUES(*ACTGRPDFN *CALLLVL *JOB) +
                          PROMPT('Open scope')
             PARM       KWD(OVRSCOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ACTGRPDFN) VALUES(*ACTGRPDFN *CALLLVL *JOB) +
                          PROMPT('Override scope')
             PARM       KWD(DSTDTA) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          VALUES(*DFT *DATA *BUFFER) +
                          PROMPT('Distributed data')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(6) RSTD(*YES) MIN(1) +
                          VALUES(*RRN *KEYB *KEYBE *KEY *KEYAE *KEYA) +
                          PROMPT('Retrieve order')
                        ELEM       TYPE(*DEC) LEN(10) RANGE(1 4294967288) +
                          PROMPT('Record number or key fields')
                        ELEM       TYPE(*NAME) LEN(10) PROMPT('Record format')
                        ELEM       TYPE(*CHAR) LEN(256) MAX(120) +
                          PROMPT('Key value')
             E2:        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Record format')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) MIN(1) +
                          VALUES(*SHRRD *SHRNUP *SHRUPD *EXCLRD *EXCL) +
                          PROMPT('Lock state')
             E3:        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) MIN(1) +
                          VALUES(*NO *YES) PROMPT('Sequential only')
                        ELEM       TYPE(*DEC) LEN(5) RANGE(1 32767) +
                          PROMPT('Records per buffer')
