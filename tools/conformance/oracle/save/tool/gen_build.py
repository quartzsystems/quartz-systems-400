# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.

"""Writes build.s: the QSTEST objects the restore cases use."""
import sys


def dds(t=' ', name='', ln='', dt='', dec='', kw='', cond=''):
    line = [' '] * 80
    line[5] = 'A'
    for i, c in enumerate(cond[:9]):
        line[7 + i] = c
    line[16] = t
    for i, c in enumerate(name[:10]):
        line[18 + i] = c
    if ln:
        s = str(ln).rjust(5)
        for i, c in enumerate(s):
            line[29 + i] = c
    if dt:
        line[34] = dt
    if dec != '':
        s = str(dec).rjust(2)
        line[35] = s[0]
        line[36] = s[1]
    for i, c in enumerate(kw):
        if 44 + i < 80:
            line[44 + i] = c
    return ''.join(line).rstrip()


def src(f, mbr, typ, lines):
    return ['src QSTEST %s %s %s' % (f, mbr, typ)] + lines + [';;']


out = []
o = out.append
o('case 01-build Objects built in QSTEST for the save and restore cases')
o('jobs')
o('note every object below is made by these commands; ZZ* objects are the capture tool\'s own work files')
o('cl CHGJOB INQMSGRPY(*DFT)')
o('cl CRTSRCPF FILE(QSTEST/QSRC) RCDLEN(112) TEXT(\'Save kit source\')')
o('cl CRTSRCPF FILE(QSTEST/QDDSSRC) RCDLEN(92) MBR(*NONE) TEXT(\'Save kit DDS\')')
out += src('QDDSSRC', 'CUST', 'PF', [
    dds('R', 'CUSTR'),
    dds(' ', 'CUSTNO', 5, 'S', 0, "COLHDG('Customer')"),
    dds(' ', 'NAME', 20, 'A'),
    dds(' ', 'CITY', 15, 'A'),
    dds(' ', 'BAL', 9, 'P', 2),
    dds('K', 'CUSTNO'),
])
out += src('QDDSSRC', 'CUSTK', 'LF', [
    dds('R', 'CUSTR', kw='PFILE(CUST)'),
    dds('K', 'NAME'),
])
out += src('QDDSSRC', 'CUSTSO', 'LF', [
    dds('R', 'CUSTR', kw='PFILE(CUST)'),
    dds('K', 'CUSTNO'),
    dds('S', 'CITY', kw="COMP(EQ 'ROCHESTER')"),
])
out += src('QDDSSRC', 'ORD', 'PF', [
    dds('R', 'ORDR'),
    dds(' ', 'ORDNO', 5, 'S', 0),
    dds(' ', 'CUSTNO', 5, 'S', 0),
    dds(' ', 'AMT', 7, 'P', 2),
    dds('K', 'ORDNO'),
])
out += src('QDDSSRC', 'CUSORD', 'LF', [
    dds('R', 'CUSORDR', kw='JFILE(CUST ORD)'),
    dds('J', kw='JOIN(CUST ORD)'),
    dds(' ', kw='JFLD(CUSTNO CUSTNO)'),
    dds(' ', 'CUSTNO', kw='JREF(1)'),
    dds(' ', 'NAME'),
    dds(' ', 'ORDNO'),
    dds(' ', 'AMT'),
    dds('K', 'CUSTNO'),
])
out += src('QDDSSRC', 'FMTA', 'PF', [
    dds('R', 'FMTR'),
    dds(' ', 'K1', 5, 'S', 0),
    dds(' ', 'F1', 10, 'A'),
])
for f in ['CUST', 'ORD', 'FMTA']:
    o('cl CRTPF FILE(QSTEST/%s) SRCFILE(QSTEST/QDDSSRC) SRCMBR(%s)' % (f, f))
for f in ['CUSTK', 'CUSTSO', 'CUSORD']:
    o('cl CRTLF FILE(QSTEST/%s) SRCFILE(QSTEST/QDDSSRC) SRCMBR(%s)' % (f, f))
o('cl CRTPF FILE(QSTEST/MPF) RCDLEN(20) MBR(M1) MAXMBRS(*NOMAX) TEXT(\'Multi-member PF\')')
o('cl ADDPFM FILE(QSTEST/MPF) MBR(M2)')
o('cl ADDPFM FILE(QSTEST/MPF) MBR(M3)')
o('cl CRTPF FILE(QSTEST/FREEPF) RCDLEN(20) TEXT(\'For STG(*FREE)\')')
o('sqlx')
o("INSERT INTO QSTEST.CUST VALUES (1, 'ACME', 'ROCHESTER', 100.50), (2, 'BETA', 'AUSTIN', 20.00), (3, 'GAMMA', 'ROCHESTER', 0), (4, 'DELTA', 'RALEIGH', -5.25), (5, 'EPSILON', 'AUSTIN', 999.99)")
o(';;')
o('sqlx')
o("INSERT INTO QSTEST.FMTA VALUES (1, 'ONE'), (2, 'TWO')")
o(';;')
o('sqlx')
o("INSERT INTO QSTEST.FREEPF VALUES ('FREE1'), ('FREE2')")
o(';;')
for m, v in [('M1', 'MEMBER1'), ('M2', 'MEMBER2'), ('M3', 'MEMBER3')]:
    o('sqlx')
    o('CREATE ALIAS QSTEST.ZZMA FOR QSTEST.MPF(%s)' % m)
    o(';;')
    o('sqlx')
    o("INSERT INTO QSTEST.ZZMA VALUES ('%s')" % v)
    o(';;')
    o('sqlx')
    o('DROP ALIAS QSTEST.ZZMA')
    o(';;')
o('sqlx')
o("CREATE TABLE QSTEST.SQLT (ID INTEGER NOT NULL PRIMARY KEY, TXT VARCHAR(30))")
o(';;')
o('sqlx')
o("INSERT INTO QSTEST.SQLT VALUES (1, 'one'), (2, 'two'), (3, 'three')")
o(';;')
o('sqlx')
o("CREATE INDEX QSTEST.SQLTIX ON QSTEST.SQLT (TXT)")
o(';;')
o('note journal, two receivers, ORD journaled')
o('cl CRTJRNRCV JRNRCV(QSTEST/RCV0001) TEXT(\'Save kit receiver\')')
o('cl CRTJRN JRN(QSTEST/JRN) JRNRCV(QSTEST/RCV0001) TEXT(\'Save kit journal\')')
o('cl STRJRNPF FILE(QSTEST/ORD) JRN(QSTEST/JRN) IMAGES(*BOTH)')
o('sqlx')
o("INSERT INTO QSTEST.ORD VALUES (100, 1, 10.00), (101, 1, 20.00), (102, 2, 30.00)")
o(';;')
o('cl CHGJRN JRN(QSTEST/JRN) JRNRCV(*GEN)')
o('sqlx')
o("INSERT INTO QSTEST.ORD VALUES (103, 5, 40.00)")
o(';;')
o('note data areas, data queue, user space, user index')
o("cl CRTDTAARA DTAARA(QSTEST/DTAC) TYPE(*CHAR) LEN(20) VALUE('HELLO') TEXT('Char data area')")
o("cl CRTDTAARA DTAARA(QSTEST/DTAD) TYPE(*DEC) LEN(9 2) VALUE(123.45) TEXT('Dec data area')")
o("cl CRTDTAARA DTAARA(QSTEST/DTAL) TYPE(*LGL) VALUE('1') TEXT('Lgl data area')")
o("cl CRTDTAQ DTAQ(QSTEST/DQ1) MAXLEN(100) TEXT('Save kit data queue')")
o("cl CALL PGM(QSNDDTAQ) PARM('DQ1' 'QSTEST' X'00010F' 'ENTRY ONE ')")
o("cl CALL PGM(QUSCRTUS) PARM('USRSPC    QSTEST    ' 'TESTSPC' X'00001000' X'00' '*ALL' 'Save kit user space                               ')")
o("cl CALL PGM(QUSCRTUI) PARM('USRIDX    QSTEST    ' 'TESTIDX' 'F' X'00000014' '1' X'00000005' '1' '0' '*ALL' 'Save kit user index                               ')")
o('note message file')
o("cl CRTMSGF MSGF(QSTEST/QSTMSGF) TEXT('Save kit messages')")
o("cl ADDMSGD MSGID(QST0001) MSGF(QSTEST/QSTMSGF) MSG('First save kit message')")
o("cl ADDMSGD MSGID(QST0002) MSGF(QSTEST/QSTMSGF) MSG('Second message for &1') FMT((*CHAR 10))")
o('note CL program, module, service program, program bound to it, command')
out += src('QSRC', 'PGM1', 'CLLE', [
    'PGM PARM(&TXT)',
    'DCL VAR(&TXT) TYPE(*CHAR) LEN(10)',
    "SNDPGMMSG MSGID(QST0002) MSGF(QSTEST/QSTMSGF) MSGDTA(&TXT) MSGTYPE(*COMP)",
    'ENDPGM',
])
out += src('QSRC', 'MOD1', 'CLLE', [
    'PGM',
    "SNDPGMMSG MSG('MOD1 called') MSGTYPE(*COMP)",
    'ENDPGM',
])
out += src('QSRC', 'MOD2', 'CLLE', [
    'PGM',
    'CALLPRC PRC(MOD1)',
    'ENDPGM',
])
out += src('QSRC', 'SRV1', 'BND', [
    'STRPGMEXP PGMLVL(*CURRENT) SIGNATURE(\'SAVEKIT SRV1 V1\')',
    'EXPORT SYMBOL(MOD1)',
    'ENDPGMEXP',
])
out += src('QSRC', 'QSTCMD', 'CMD', [
    "CMD PROMPT('Save kit command')",
    "PARM KWD(TXT) TYPE(*CHAR) LEN(10) DFT(DEFAULT) PROMPT('Text')",
])
o('cl CRTBNDCL PGM(QSTEST/PGM1) SRCFILE(QSTEST/QSRC) SRCMBR(PGM1)')
o('cl CRTCLMOD MODULE(QSTEST/MOD1) SRCFILE(QSTEST/QSRC) SRCMBR(MOD1)')
o('cl CRTCLMOD MODULE(QSTEST/MOD2) SRCFILE(QSTEST/QSRC) SRCMBR(MOD2)')
o('cl CRTSRVPGM SRVPGM(QSTEST/SRV1) MODULE(QSTEST/MOD1) EXPORT(*SRCFILE) SRCFILE(QSTEST/QSRC) SRCMBR(SRV1)')
o('cl CRTPGM PGM(QSTEST/PGM2) MODULE(QSTEST/MOD2) BNDSRVPGM(QSTEST/SRV1) ACTGRP(*NEW)')
o('cl CRTCMD CMD(QSTEST/QSTCMD) PGM(QSTEST/PGM1) SRCFILE(QSTEST/QSRC) SRCMBR(QSTCMD)')
o('cl QSTEST/QSTCMD TXT(BUILT)')
o('cl CALL PGM(QSTEST/PGM2)')
o('note job description, output queue with three spooled files')
o("cl CRTJOBD JOBD(QSTEST/QSTJOBD) TEXT('Save kit job description') INLLIBL(QSTEST QGPL QTEMP)")
o("cl CRTOUTQ OUTQ(QSTEST/QSTOUTQ) TEXT('Save kit output queue')")
o('cl OVRPRTF FILE(*PRTF) OUTQ(QSTEST/QSTOUTQ) OVRSCOPE(*JOB)')
o('cl DSPLIB LIB(QSTEST) OUTPUT(*PRINT)')
o('cl DSPOBJD OBJ(QSTEST/*ALL) OBJTYPE(*ALL) OUTPUT(*PRINT)')
o('cl DSPFD FILE(QSTEST/CUST) TYPE(*BASATR) OUTPUT(*PRINT)')
o('cl DLTOVR FILE(*PRTF) LVL(*JOB)')
o('splf wrkoutq-qstoutq WRKOUTQ OUTQ(QSTEST/QSTOUTQ) OUTPUT(*PRINT)')
o('note private authorities, owner, primary group')
o('cl GRTOBJAUT OBJ(QSTEST/CUST) OBJTYPE(*FILE) USER(USRA) AUT(*CHANGE)')
o('cl GRTOBJAUT OBJ(QSTEST/DTAC) OBJTYPE(*DTAARA) USER(USRA) AUT(*USE)')
o('cl GRTOBJAUT OBJ(QSTEST/PGM1) OBJTYPE(*PGM) USER(USRA) AUT(*EXCLUDE)')
o('cl GRTOBJAUT OBJ(QSTEST/DTAD) OBJTYPE(*DTAARA) USER(*PUBLIC) AUT(*EXCLUDE)')
o('cl CHGOBJOWN OBJ(QSTEST/DTAD) OBJTYPE(*DTAARA) NEWOWN(OWNA)')
o('cl CHGOBJPGP OBJ(QSTEST/DTAL) OBJTYPE(*DTAARA) NEWPGP(GRPA)')
for obj, t in [('CUST', '*FILE'), ('DTAC', '*DTAARA'), ('DTAD', '*DTAARA'), ('DTAL', '*DTAARA'), ('PGM1', '*PGM')]:
    o('splf dspobjaut-%s DSPOBJAUT OBJ(QSTEST/%s) OBJTYPE(%s) OUTPUT(*PRINT)' % (obj.lower(), obj, t))
o('sql inventory')
o("SELECT OBJNAME, OBJTYPE, OBJOWNER, OBJSIZE, OBJTEXT FROM TABLE(QSYS2.OBJECT_STATISTICS('QSTEST', '*ALL')) X ORDER BY 1, 2")
o(';;')
o('splf dsplib DSPLIB LIB(QSTEST) OUTPUT(*PRINT)')
o('splf dspfd-mpf DSPFD FILE(QSTEST/MPF) TYPE(*MBRLIST) OUTPUT(*PRINT)')
o('splf wrkjrna WRKJRNA JRN(QSTEST/JRN) OUTPUT(*PRINT)')
open(sys.argv[1], 'w').write('\n'.join(out) + '\n')
