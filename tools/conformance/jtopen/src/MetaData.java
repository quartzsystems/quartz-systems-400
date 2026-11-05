// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.Connection;
import java.sql.DatabaseMetaData;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;
import java.sql.Statement;

/**
 * DatabaseMetaData over jdbc:as400 for a small table set made in QSTEST: a
 * parent table with a primary key, a child with a foreign key, a view and an
 * index. Every result set is printed whole (column labels, JDBC types and rows),
 * so a run against QS/400 diffs line for line with the oracle's. The tables are
 * dropped at the end.
 */
public final class MetaData {
    private static final String LIB = "QSTEST";

    private static Connection open() throws Exception {
        String url = "jdbc:as400://" + Hs.host() + (Hs.skipSignon() ? ":8471" : "")
            + ";naming=sql;prompt=false;errors=full;libraries=" + LIB;
        return DriverManager.getConnection(url, Hs.user(), Hs.pass());
    }

    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        try (Connection c = open()) {
            Hs.out("ok   connect");
            drop(c);
            Hs.step("create", () -> {
                exec(c, "CREATE TABLE " + LIB + ".QSMDP (ID INTEGER NOT NULL PRIMARY KEY, NAME VARCHAR(30) NOT NULL, "
                    + "AMT DECIMAL(9, 2) DEFAULT 0, D DATE, C CHAR(4))");
                exec(c, "CREATE TABLE " + LIB + ".QSMDC (CID INTEGER NOT NULL, PID INTEGER NOT NULL, NOTE VARCHAR(10), "
                    + "PRIMARY KEY (CID), FOREIGN KEY (PID) REFERENCES " + LIB + ".QSMDP (ID) ON DELETE CASCADE)");
                exec(c, "CREATE VIEW " + LIB + ".QSMDV AS SELECT ID, NAME FROM " + LIB + ".QSMDP");
                exec(c, "CREATE UNIQUE INDEX " + LIB + ".QSMDX ON " + LIB + ".QSMDP (NAME DESC, ID)");
                exec(c, "LABEL ON TABLE " + LIB + ".QSMDP IS 'Metadata parent'");
            });
            DatabaseMetaData m = c.getMetaData();
            Hs.step("scalars", () -> {
                Hs.out("     product=" + m.getDatabaseProductName() + " " + m.getDatabaseProductVersion().replaceAll("\\d", "n"));
                Hs.out("     quote=" + m.getIdentifierQuoteString() + " catalogSep=" + m.getCatalogSeparator()
                    + " catalogTerm=" + m.getCatalogTerm() + " schemaTerm=" + m.getSchemaTerm() + " procTerm=" + m.getProcedureTerm());
                Hs.out("     maxColumnName=" + m.getMaxColumnNameLength() + " maxTableName=" + m.getMaxTableNameLength()
                    + " maxSchemaName=" + m.getMaxSchemaNameLength() + " maxStatement=" + m.getMaxStatementLength());
                Hs.out("     isolation=" + m.getDefaultTransactionIsolation() + " batch=" + m.supportsBatchUpdates()
                    + " savepoints=" + m.supportsSavepoints() + " generatedKeys=" + m.supportsGetGeneratedKeys());
                Hs.out("     keywords=" + m.getSQLKeywords());
                Hs.out("     numeric=" + m.getNumericFunctions());
                Hs.out("     string=" + m.getStringFunctions());
                Hs.out("     time=" + m.getTimeDateFunctions());
            });
            Hs.step("getTables", () -> dump(m.getTables(null, LIB, "QSMD%", null)));
            Hs.step("getTables views", () -> dump(m.getTables(null, LIB, "QSMD%", new String[] {"VIEW"})));
            Hs.step("getColumns", () -> dump(m.getColumns(null, LIB, "QSMD%", "%")));
            Hs.step("getPrimaryKeys", () -> dump(m.getPrimaryKeys(null, LIB, "QSMDP")));
            Hs.step("getImportedKeys", () -> dump(m.getImportedKeys(null, LIB, "QSMDC")));
            Hs.step("getExportedKeys", () -> dump(m.getExportedKeys(null, LIB, "QSMDP")));
            Hs.step("getCrossReference", () -> dump(m.getCrossReference(null, LIB, "QSMDP", null, LIB, "QSMDC")));
            Hs.step("getIndexInfo", () -> dump(m.getIndexInfo(null, LIB, "QSMDP", false, false)));
            Hs.step("getIndexInfo unique", () -> dump(m.getIndexInfo(null, LIB, "QSMDP", true, false)));
            Hs.step("getBestRowIdentifier", () -> dump(m.getBestRowIdentifier(null, LIB, "QSMDP", DatabaseMetaData.bestRowSession, true)));
            Hs.step("getVersionColumns", () -> dump(m.getVersionColumns(null, LIB, "QSMDP")));
            Hs.step("getTablePrivileges", () -> dump(m.getTablePrivileges(null, LIB, "QSMDP")));
            Hs.step("getColumnPrivileges", () -> dump(m.getColumnPrivileges(null, LIB, "QSMDP", "%")));
            Hs.step("getSchemas", () -> dump(m.getSchemas(null, LIB)));
            Hs.step("getCatalogs", () -> dump(m.getCatalogs()));
            Hs.step("getTableTypes", () -> dump(m.getTableTypes()));
            Hs.step("getTypeInfo", () -> dump(m.getTypeInfo()));
            Hs.step("getProcedures", () -> dump(m.getProcedures(null, LIB, "QSMD%")));
            Hs.step("getProcedureColumns", () -> dump(m.getProcedureColumns(null, LIB, "QSMD%", "%")));
            Hs.step("getFunctions", () -> dump(m.getFunctions(null, LIB, "QSMD%")));
            Hs.step("getUDTs", () -> dump(m.getUDTs(null, LIB, "QSMD%", null)));
            Hs.step("drop", () -> drop(c));
        }
    }

    private static void exec(Connection c, String sql) throws SQLException {
        try (Statement s = c.createStatement()) {
            s.execute(sql);
        }
    }

    private static void drop(Connection c) {
        for (String o : new String[] {"VIEW QSMDV", "INDEX QSMDX", "TABLE QSMDC", "TABLE QSMDP"}) {
            try {
                exec(c, "DROP " + o.replace(" ", " " + LIB + "."));
            } catch (SQLException e) {
                // Not there.
            }
        }
    }

    /** A result set: its columns, then each row; the system's name and the user written as placeholders. */
    private static void dump(ResultSet r) throws SQLException {
        try (r) {
            ResultSetMetaData md = r.getMetaData();
            int n = md.getColumnCount();
            StringBuilder h = new StringBuilder("     #");
            for (int i = 1; i <= n; i++) {
                h.append(' ').append(md.getColumnLabel(i)).append(':').append(md.getColumnTypeName(i));
            }
            Hs.out(h.toString());
            int rows = 0;
            while (r.next()) {
                StringBuilder b = new StringBuilder("     |");
                for (int i = 1; i <= n; i++) {
                    String v = r.getString(i);
                    b.append(' ').append(v == null ? "NULL" : v.trim()).append(" |");
                }
                Hs.out(b.toString().replace(Hs.user().toUpperCase(), "{user}"));
                rows++;
            }
            Hs.out("     rows=" + rows);
        }
    }
}
