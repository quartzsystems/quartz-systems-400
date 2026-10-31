case p75-survey pub400 account library before the round trip
sql objects in the account library
SELECT OBJNAME, OBJTYPE, OBJOWNER, OBJSIZE, OBJTEXT FROM TABLE(QSYS2.OBJECT_STATISTICS('{work}', '*ALL')) X ORDER BY 1, 2
;;
sql my spooled files
SELECT COUNT(*) FROM QSYS2.OUTPUT_QUEUE_ENTRIES_BASIC WHERE USER_NAME = CURRENT USER
;;
