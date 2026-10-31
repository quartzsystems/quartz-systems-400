case p75-roundtrip SAVLIB and RSTLIB of the account library on pub400
boot
jobs
note ZZ* are the capture tool's work files, created for this run and deleted after it
cl CRTSAVF FILE({work}/QSSAVF) TEXT('Phase 11 round trip')
cl SAVLIB LIB({work}) DEV(*SAVF) SAVF({work}/QSSAVF) OMITOBJ(({work}/ZZ*) ({work}/QSSAVF))
splf dspsavf DSPSAVF FILE({work}/QSSAVF) OUTPUT(*PRINT)
