-- oracle: primary
-- Delimited names: quoted system names, generated quoted names.
CREATE TABLE {lib}."ddllow" (X INT);
CREATE TABLE {lib}."ddl long lower name" ("a column" INT);
SELECT TABLE_NAME, SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = '{lib}' AND TABLE_NAME LIKE 'ddl%' ORDER BY TABLE_NAME;
SELECT COLUMN_NAME, SYSTEM_COLUMN_NAME FROM QSYS2.SYSCOLUMNS WHERE TABLE_SCHEMA = '{lib}' AND TABLE_NAME = 'ddl long lower name';
INSERT INTO {lib}."ddl long lower name" VALUES (5);
SELECT "a column" FROM {lib}."ddl long lower name";
DROP TABLE {lib}."ddllow";
DROP TABLE {lib}."ddl long lower name";
