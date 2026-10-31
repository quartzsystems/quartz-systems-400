case 31-dspptf DSPPTF in print form
splf dspptf-one DSPPTF LICPGM(5770SS1) SELECT(SI42904) OUTPUT(*PRINT)
splf dspptfcvr-one DSPPTFCVR LICPGM(5770SS1) SELECT(SI42904) OUTPUT(*PRINT)
splf dspptfcvr-all DSPPTFCVR LICPGM(5770SS1) SELECT(*ALL) OUTPUT(*PRINT)
case 33-ipla IPL attributes, service attributes
splf dspbckup-daily DSPBCKUP BCKUPOPT(*DAILY)
splf dspbckupl DSPBCKUPL OUTPUT(*PRINT)
case 34-devices Tape, optical and media library devices, image catalogs
splf dspdevd-tap01 DSPDEVD DEVD(TAP01) OUTPUT(*PRINT)
splf dspdevd-opt01 DSPDEVD DEVD(OPT01) OUTPUT(*PRINT)
splf dspdevd-optvrt01 DSPDEVD DEVD(OPTVRT01) OUTPUT(*PRINT)
splf dsptap-tap01 DSPTAP DEV(TAP01) OUTPUT(*PRINT)
splf dspopt-optvrt01 DSPOPT VOL(*MOUNTED) DEV(OPTVRT01) DATA(*VOLATR) OUTPUT(*PRINT)
splf dspopt-opt01 DSPOPT VOL(*MOUNTED) DEV(OPT01) DATA(*VOLATR) OUTPUT(*PRINT)
splf wrkimgclge-java8 WRKIMGCLGE IMGCLG(JAVA8) OUTPUT(*PRINT)
splf dspobjd-imgclg DSPOBJD OBJ(QUSRSYS/JAVA8) OBJTYPE(*IMGCLG) DETAIL(*FULL) OUTPUT(*PRINT)
case 35-problems Problems, licenses, contact
splf dspprb DSPPRB OUTPUT(*PRINT)
