// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;
import java.sql.SQLWarning;
import java.sql.Statement;
import java.util.ArrayList;
import java.util.List;

/**
 * Runs a conformance case (tools/conformance/sql/cases/.../NAME.sql) on an oracle over
 * jdbc:as400 and prints, per statement, the SQLCODE, SQLSTATE and message ID, the row
 * count, and any result set's columns and rows. Usage: SqlProbe primary|pub400 FILE...
 * Credentials come from QS_ORACLE_* or QS_PUB400_*; {lib} is QSTEST on the primary and
 * QS_PUB400_LIB on pub400. The account name prints as {user}.
 */
public final class SqlProbe {
    private static String user = "";

    private static String env(String k) {
        String v = System.getenv(k);
        if (v == null || v.isEmpty()) {
            throw new IllegalStateException(k + " is not set");
        }
        return v;
    }

    private static String mask(String s) {
        if (s == null) {
            return "-";
        }
        return user.isEmpty() ? s : s.replace(user.toUpperCase(), "{user}").replace(user.toLowerCase(), "{user}");
    }

    /** The statements of a case, ';' (or the --#SET TERMINATOR) at end of line. */
    static List<String> statements(String text) {
        List<String> out = new ArrayList<>();
        String term = ";";
        StringBuilder cur = new StringBuilder();
        for (String line : text.split("\r?\n")) {
            String t = line.trim();
            if (t.startsWith("--#SET TERMINATOR")) {
                term = t.substring("--#SET TERMINATOR".length()).trim();
                continue;
            }
            if (t.startsWith("--") && cur.length() == 0) {
                continue;
            }
            if (t.endsWith(term)) {
                cur.append(line, 0, line.lastIndexOf(term));
                String s = cur.toString().trim();
                if (!s.isEmpty()) {
                    out.add(s);
                }
                cur.setLength(0);
            } else {
                cur.append(line).append('\n');
            }
        }
        if (!cur.toString().trim().isEmpty()) {
            out.add(cur.toString().trim());
        }
        return out;
    }

    private static String msgid(String text) {
        if (text != null && text.startsWith("[") && text.indexOf(']') > 0) {
            return text.substring(1, text.indexOf(']'));
        }
        return "-";
    }

    private static String hex(byte[] b) {
        StringBuilder s = new StringBuilder();
        for (byte x : b) {
            s.append(String.format("%02X", x));
        }
        return s.toString();
    }

    private static void run(Connection c, String sql) throws SQLException {
        System.out.println("> " + mask(sql.replace('\n', ' ')));
        try (Statement s = c.createStatement()) {
            boolean rs = s.execute(sql);
            SQLWarning w = s.getWarnings();
            while (w != null) {
                System.out.println("  warning " + w.getErrorCode() + " " + w.getSQLState() + " " + msgid(w.getMessage()) + " " + mask(w.getMessage()));
                w = w.getNextWarning();
            }
            if (!rs) {
                System.out.println("  sqlcode 0 count " + s.getUpdateCount());
                return;
            }
            try (ResultSet r = s.getResultSet()) {
                ResultSetMetaData m = r.getMetaData();
                StringBuilder h = new StringBuilder("  cols");
                for (int i = 1; i <= m.getColumnCount(); i++) {
                    h.append(" | ").append(m.getColumnName(i)).append(' ').append(m.getColumnTypeName(i))
                        .append('(').append(m.getPrecision(i)).append(',').append(m.getScale(i)).append(')')
                        .append(m.isNullable(i) == ResultSetMetaData.columnNoNulls ? " NN" : "");
                }
                System.out.println(h);
                int n = 0;
                while (r.next() && n < 200) {
                    StringBuilder row = new StringBuilder("  row");
                    for (int i = 1; i <= m.getColumnCount(); i++) {
                        String tn = m.getColumnTypeName(i);
                        String v;
                        if (tn.contains("BINARY") || tn.equals("BLOB") || tn.equals("ROWID")) {
                            byte[] b = r.getBytes(i);
                            v = b == null ? "-" : "x'" + hex(b) + "'";
                        } else {
                            String x = r.getString(i);
                            v = x == null ? "-" : "'" + x + "'";
                        }
                        row.append(" | ").append(mask(v));
                    }
                    System.out.println(row);
                    n++;
                }
                SQLWarning fw = r.getWarnings();
                while (fw != null) {
                    System.out.println("  warning " + fw.getErrorCode() + " " + fw.getSQLState() + " " + msgid(fw.getMessage()));
                    fw = fw.getNextWarning();
                }
            } catch (SQLException e) {
                System.out.println("  fetch sqlcode " + e.getErrorCode() + " " + e.getSQLState() + " " + msgid(e.getMessage()) + " " + mask(e.getMessage()));
            }
        } catch (SQLException e) {
            System.out.println("  sqlcode " + e.getErrorCode() + " " + e.getSQLState() + " " + msgid(e.getMessage()) + " " + mask(e.getMessage()));
        }
    }

    public static void main(String[] a) throws Exception {
        String target = a[0];
        String p = target.equals("pub400") ? "QS_PUB400_" : "QS_ORACLE_";
        user = env(p + "USER");
        String lib = target.equals("pub400") ? env("QS_PUB400_LIB") : "QSTEST";
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + env(p + "HOST") + ";naming=sql;prompt=false;errors=full;libraries=" + lib
            + ";translate binary=false";
        try (Connection c = DriverManager.getConnection(url, user, env(p + "PASS"))) {
            c.setAutoCommit(true);
            try (Statement s = c.createStatement()) {
                s.execute("SET SCHEMA " + lib);
            }
            for (int i = 1; i < a.length; i++) {
                System.out.println("# " + Path.of(a[i]).getFileName());
                for (String sql : statements(Files.readString(Path.of(a[i])))) {
                    run(c, sql.replace("{lib}", lib));
                }
            }
        }
    }
}
