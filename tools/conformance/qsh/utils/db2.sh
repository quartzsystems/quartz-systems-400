# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# db2: result set layout, counts, messages, errors, -t -v -f -S, statuses.
# A table is made only in the test library and dropped.

# QSHUTILS_LIB names the library objects are made in (QSTEST; on
# pub400 the account's own).
L=${QSHUTILS_LIB:-QSTEST}

probe select 'db2 "select table_name, table_type from qsys2.systables where table_schema = '\''QSYS2'\'' and table_name like '\''SYSTAB%'\'' order by 1"'
probe count 'db2 "select count(*) from qsys2.sysschemas where schema_name = '\''QSYS'\''"'
probe null 'db2 "values (cast(null as int), '\''x'\'', 1.5)"'
probe empty 'db2 "select * from qsys2.sysschemas where 1 = 0"'
probe ddl 'db2 "create table $L.qshprb (a int, b varchar(10))"; echo $?; db2 "insert into $L.qshprb values(1, '\''x'\'')"; echo $?; db2 "select * from $L.qshprb"; db2 "drop table $L.qshprb"; echo $?'
probe error 'db2 "select * from $L.nosuchtbl"; echo $?'
probe syntax 'db2 "frobnicate"; echo $?'
probe system-naming 'db2 -S "select count(*) from qsys2/sysschemas"; echo $?'
probe stdin 'printf '\''values 1\nvalues 2\n'\'' | db2'
probe terminator 'printf '\''values 1;\nvalues\n 2;'\'' | db2 -t'
probe verbose 'db2 -v "values 1"'
probe file 'printf '\''values 3\n'\'' > s.sql; db2 -f s.sql'
probe usage 'db2 -z x; echo $?'
finish
