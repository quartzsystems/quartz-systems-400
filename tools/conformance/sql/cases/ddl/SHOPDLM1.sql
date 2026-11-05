-- oracle: primary
-- Delimited names in a shop schema: mixed case and blanks in table and column names, quoting in queries, the catalog's names.
CREATE TABLE {lib}."D4 Monthly Totals" ("Month" CHAR(7) NOT NULL, "Total Sales" DECIMAL(11, 2), "units" INTEGER, PRIMARY KEY ("Month"));
INSERT INTO {lib}."D4 Monthly Totals" VALUES ('2026-01', 100.00, 5), ('2026-02', 250.50, 9);
SELECT "Month", "Total Sales", "units" FROM {lib}."D4 Monthly Totals" ORDER BY "Month";
SELECT * FROM {lib}."D4 Monthly Totals" WHERE "units" > 6;
SELECT "Month", UNITS FROM {lib}."D4 Monthly Totals";
SELECT COUNT(*) FROM {lib}."d4 monthly totals";
CREATE VIEW {lib}."D4 Big Months" AS SELECT "Month" AS "Big Month" FROM {lib}."D4 Monthly Totals" WHERE "Total Sales" > 200;
SELECT * FROM {lib}."D4 Big Months";
SELECT TABLE_NAME, SYSTEM_TABLE_NAME, TABLE_TYPE FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = '{lib}' AND TABLE_NAME LIKE 'D4 %' ORDER BY TABLE_NAME;
SELECT COLUMN_NAME, SYSTEM_COLUMN_NAME FROM QSYS2.SYSCOLUMNS WHERE TABLE_SCHEMA = '{lib}' AND TABLE_NAME = 'D4 Monthly Totals' ORDER BY ORDINAL_POSITION;
DROP VIEW {lib}."D4 Big Months";
DROP TABLE {lib}."D4 Monthly Totals";
