// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.CallableStatement;
import java.sql.Connection;
import java.sql.DatabaseMetaData;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.Statement;
import java.sql.Timestamp;

/**
 * Database server over jdbc:as400: connect, metadata, prepare/execute/fetch with
 * blocked fetch over a QSYS2 catalog view and over a table made for the run in
 * QSTEST (QTEMP when QSTEST does not exist), an SQL error, commit and rollback,
 * a procedure call. The table is dropped at the end.
 */
public final class Jdbc {
    /** The URL's connection, or through a system object when the database port is moved. */
    private static Connection open(String url) throws Exception {
        String moved = System.getenv("QS_HS_PORT_8471");
        if (moved == null || moved.isEmpty()) {
            return DriverManager.getConnection(url, Hs.user(), Hs.pass());
        }
        java.util.Properties p = new java.util.Properties();
        p.setProperty("naming", "sql");
        p.setProperty("prompt", "false");
        p.setProperty("block size", "8");
        p.setProperty("errors", "full");
        return new com.ibm.as400.access.AS400JDBCDriver().connect(Hs.signonSystem(), p, null);
    }

    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + Hs.host() + (Hs.skipSignon() ? ":8471" : "") + ";naming=sql;prompt=false;block size=8;errors=full";
        try (Connection c = open(url)) {
            Hs.out("ok   connect");
            Hs.step("metadata", () -> metadata(c));
            Hs.step("catalog fetch", () -> catalog(c));
            String[] lib = {"QTEMP"};
            Hs.step("find QSTEST", () -> {
                try (Statement s = c.createStatement();
                     ResultSet r = s.executeQuery("SELECT COUNT(*) FROM QSYS2.SYSSCHEMAS WHERE SCHEMA_NAME = 'QSTEST'")) {
                    r.next();
                    if (r.getInt(1) > 0) {
                        lib[0] = "QSTEST";
                    }
                }
                Hs.out("     library=" + lib[0]);
            });
            String t = lib[0] + ".HSTRACE1";
            Hs.step("create", () -> exec(c, "CREATE TABLE " + t
                + " (ID INTEGER NOT NULL, NAME VARCHAR(20), AMT DECIMAL(9, 2), TS TIMESTAMP, NOTE CHAR(10))"));
            Hs.step("insert batch", () -> insert(c, t));
            Hs.step("select all", () -> select(c, "SELECT * FROM " + t + " ORDER BY ID", 3));
            Hs.step("select where", () -> {
                try (PreparedStatement p = c.prepareStatement("SELECT ID, AMT FROM " + t + " WHERE ID BETWEEN ? AND ? ORDER BY ID")) {
                    p.setInt(1, 10);
                    p.setInt(2, 14);
                    dump(p.executeQuery(), 10);
                }
            });
            Hs.step("update", () -> Hs.out("     rows=" + update(c, "UPDATE " + t + " SET NOTE = 'CHANGED' WHERE MOD(ID, 7) = 0")));
            Hs.step("delete", () -> Hs.out("     rows=" + update(c, "DELETE FROM " + t + " WHERE ID > 250")));
            Hs.step("count", () -> select(c, "SELECT COUNT(*), SUM(AMT), MAX(NAME) FROM " + t, 1));
            Hs.step("null and types", () -> select(c,
                "SELECT CAST(NULL AS INTEGER), CAST(1.5 AS DOUBLE), CAST('x' AS CHAR(3)), DATE('2026-01-02'), BIGINT(9) FROM SYSIBM.SYSDUMMY1", 1));
            Hs.step("missing table", () -> select(c, "SELECT * FROM " + lib[0] + ".HSNOSUCH1", 1));
            Hs.step("commit and rollback", () -> {
                c.setAutoCommit(false);
                c.commit();
                c.rollback();
                c.setAutoCommit(true);
            });
            Hs.step("procedure", () -> {
                try (CallableStatement p = c.prepareCall("CALL QSYS.QCMDEXC(?, ?)")) {
                    String cmd = "CHKOBJ OBJ(QSYS/QSYSOPR) OBJTYPE(*MSGQ)";
                    p.setString(1, cmd);
                    p.setBigDecimal(2, new java.math.BigDecimal(cmd.length()).setScale(5));
                    p.execute();
                }
            });
            Hs.step("drop", () -> exec(c, "DROP TABLE " + t));
        }
    }

    static void exec(Connection c, String sql) throws Exception {
        try (Statement s = c.createStatement()) {
            s.execute(sql);
        }
    }

    static int update(Connection c, String sql) throws Exception {
        try (Statement s = c.createStatement()) {
            return s.executeUpdate(sql);
        }
    }

    static void metadata(Connection c) throws Exception {
        DatabaseMetaData m = c.getMetaData();
        Hs.out("     product=" + m.getDatabaseProductName() + " version=" + m.getDatabaseProductVersion());
        try (ResultSet r = m.getTables(null, "QSYS2", "SYSTAB%", null)) {
            dump(r, 5);
        }
        try (ResultSet r = m.getColumns(null, "QSYS2", "SYSTABLES", "TABLE_%")) {
            dump(r, 3);
        }
    }

    static void catalog(Connection c) throws Exception {
        try (PreparedStatement p = c.prepareStatement(
                "SELECT TABLE_NAME, TABLE_TYPE, COLUMN_COUNT FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME FETCH FIRST 400 ROWS ONLY")) {
            p.setString(1, "QSYS2");
            dump(p.executeQuery(), 3);
        }
    }

    static void insert(Connection c, String t) throws Exception {
        try (PreparedStatement p = c.prepareStatement("INSERT INTO " + t + " VALUES (?, ?, ?, ?, ?)")) {
            for (int i = 1; i <= 300; i++) {
                p.setInt(1, i);
                p.setString(2, "ROW " + i);
                p.setBigDecimal(3, new java.math.BigDecimal(i).movePointLeft(2).add(new java.math.BigDecimal(1000)));
                p.setTimestamp(4, Timestamp.valueOf("2026-10-24 12:00:00." + String.format("%03d", i)));
                if (i % 3 == 0) {
                    p.setNull(5, java.sql.Types.CHAR);
                } else {
                    p.setString(5, "N" + i);
                }
                p.addBatch();
            }
            int[] n = p.executeBatch();
            Hs.out("     batches=" + n.length);
        }
    }

    static void select(Connection c, String sql, int show) throws Exception {
        try (Statement s = c.createStatement()) {
            dump(s.executeQuery(sql), show);
        }
    }

    /** The column names, the first `show` rows, and the row count. */
    static void dump(ResultSet r, int show) throws Exception {
        try (r) {
            ResultSetMetaData m = r.getMetaData();
            int n = m.getColumnCount();
            StringBuilder h = new StringBuilder("     cols");
            for (int i = 1; i <= n; i++) {
                h.append(' ').append(m.getColumnName(i)).append(':').append(m.getColumnTypeName(i));
            }
            Hs.out(h.toString());
            int rows = 0;
            while (r.next()) {
                if (rows < show) {
                    StringBuilder b = new StringBuilder("     row");
                    for (int i = 1; i <= n; i++) {
                        b.append(" |").append(r.getString(i));
                    }
                    Hs.out(b.toString());
                }
                rows++;
            }
            Hs.out("     rows=" + rows);
        }
    }
}
