// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;
import java.sql.SQLWarning;
import java.sql.Statement;
import java.sql.Types;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * The SQL kit's oracle side: runs case files over jdbc:as400 against the
 * primary or pub400 and writes each case's outcome in the kit's form
 * (tools/conformance/sql/README.md), the same form the Rust side writes.
 *
 *   java -cp CLASSES:jt400.jar SqlKit primary|pub400 OUTDIR CASE.sql...
 *
 * Credentials come from the environment or tools/conformance/oracle/.env
 * (QS_ORACLE_ENV names another file). The library a case writes in and
 * the account name are written as {lib} and {user}.
 */
public final class SqlKit {
    private static Map<String, String> env() throws IOException {
        Map<String, String> m = new HashMap<>(System.getenv());
        String f = m.getOrDefault("QS_ORACLE_ENV", "tools/conformance/oracle/.env");
        Path p = Path.of(f);
        if (Files.exists(p)) {
            for (String line : Files.readAllLines(p, StandardCharsets.UTF_8)) {
                int eq = line.indexOf('=');
                if (eq > 0 && !line.startsWith("#")) {
                    m.putIfAbsent(line.substring(0, eq).trim(), line.substring(eq + 1).trim());
                }
            }
        }
        return m;
    }

    public static void main(String[] a) throws Exception {
        if (a.length < 3) {
            System.err.println("usage: SqlKit primary|pub400 OUTDIR CASE.sql...");
            System.exit(2);
        }
        Map<String, String> e = env();
        String pre = a[0].equals("pub400") ? "QS_PUB400_" : "QS_ORACLE_";
        String host = e.get(pre + "HOST");
        String user = e.get(pre + "USER");
        String pass = e.get(pre + "PASS");
        if (host == null || user == null || pass == null) {
            throw new IllegalStateException("no " + pre + "* credentials");
        }
        // pub400's writable library is the user name plus 1 unless QS_PUB400_LIB says otherwise.
        String lib = a[0].equals("pub400")
            ? e.getOrDefault("QS_PUB400_LIB", user.toUpperCase() + "1").toUpperCase()
            : "QSTEST";
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String port = "1".equals(e.get("QS_SQL_DIRECT")) ? ":8471" : "";
        String url = "jdbc:as400://" + host + port + ";naming=sql;prompt=false;errors=full"
            + ";date format=iso;time format=iso;decimal separator=.;libraries=" + lib;
        Path out = Path.of(a[1]);
        for (int i = 2; i < a.length; i++) {
            Path c = Path.of(a[i]);
            String area = c.getParent().getFileName().toString();
            String name = c.getFileName().toString().replaceFirst("\\.sql$", "");
            List<String> stmts = Case.statements(Files.readString(c, StandardCharsets.UTF_8));
            StringBuilder sb = new StringBuilder();
            sb.append("-- captured from ").append(a[0]).append('\n');
            String server = null;
            try (Connection conn = DriverManager.getConnection(url, user, pass)) {
                try (Statement s = conn.createStatement()) {
                    s.execute("SET SCHEMA " + lib);
                    try (ResultSet r = s.executeQuery("VALUES CURRENT SERVER")) {
                        server = r.next() ? r.getString(1).trim() : null;
                    }
                }
                for (String st : stmts) {
                    run(conn, st.replace("{lib}", lib), st, sb);
                }
            }
            String text = sb.toString().replace(lib, "{lib}").replace(user.toUpperCase(), "{user}");
            // The server's name as a value (CURRENT SERVER, the SYSIBM views' catalog).
            if (server != null && !server.isEmpty()) {
                text = text.replace("'" + server + "'", "'{server}'");
            }
            Path dir = out.resolve(area);
            Files.createDirectories(dir);
            Files.writeString(dir.resolve(name + ".out"), text, StandardCharsets.UTF_8);
            System.out.println("ok   " + area + "/" + name);
            Thread.sleep(500);
        }
    }

    private static void run(Connection c, String sql, String shown, StringBuilder sb) {
        sb.append("> ").append(Case.first(shown)).append('\n');
        try (Statement s = c.createStatement()) {
            boolean rows = s.execute(sql);
            int count = rows ? -1 : s.getUpdateCount();
            List<String> warn = new ArrayList<>();
            for (SQLWarning w = s.getWarnings(); w != null; w = w.getNextWarning()) {
                warn.add(w.getErrorCode() + " " + w.getSQLState() + " " + Case.msgid(w));
            }
            sb.append(Case.status(0, "00000", "-", Case.changes(shown) ? count : -1)).append('\n');
            for (String w : warn) {
                sb.append("warning ").append(w).append('\n');
            }
            if (rows) {
                try (ResultSet r = s.getResultSet()) {
                    dump(r, sb);
                }
            }
        } catch (SQLException x) {
            sb.append(Case.status(x.getErrorCode(), x.getSQLState(), Case.msgid(x), -1)).append('\n');
        }
    }

    private static void dump(ResultSet r, StringBuilder sb) throws SQLException {
        ResultSetMetaData m = r.getMetaData();
        int n = m.getColumnCount();
        for (int i = 1; i <= n; i++) {
            sb.append("# ").append(m.getColumnLabel(i)).append(' ')
                .append(Case.type(m.getColumnTypeName(i), m.getPrecision(i), m.getScale(i)))
                .append(m.isNullable(i) == ResultSetMetaData.columnNoNulls ? " NOT NULL" : "")
                .append('\n');
        }
        while (r.next()) {
            StringBuilder row = new StringBuilder("|");
            for (int i = 1; i <= n; i++) {
                row.append(' ').append(Case.value(r, i, m.getColumnType(i))).append(" |");
            }
            sb.append(row).append('\n');
        }
    }

    /** The kit's text rules, shared with the Rust side (tools/conformance/src/sql/render.rs). */
    static final class Case {
        static List<String> statements(String text) {
            List<String> out = new ArrayList<>();
            String term = ";";
            StringBuilder cur = new StringBuilder();
            for (String line : text.split("\r?\n", -1)) {
                String t = line.trim();
                if (t.toUpperCase().startsWith("--#SET TERMINATOR")) {
                    term = t.substring("--#SET TERMINATOR".length()).trim();
                    continue;
                }
                if (cur.length() == 0 && (t.isEmpty() || t.startsWith("--"))) {
                    continue;
                }
                if (t.endsWith(term)) {
                    cur.append(line, 0, line.lastIndexOf(term));
                    out.add(cur.toString().trim());
                    cur.setLength(0);
                } else {
                    cur.append(line).append('\n');
                }
            }
            if (cur.toString().trim().length() > 0) {
                out.add(cur.toString().trim());
            }
            return out;
        }

        static String first(String st) {
            String l = st.split("\n", 2)[0].trim();
            return l.length() > 72 ? l.substring(0, 72) : l;
        }

        static boolean changes(String st) {
            String w = st.trim().split("\\s+", 2)[0].toUpperCase();
            return w.equals("INSERT") || w.equals("UPDATE") || w.equals("DELETE") || w.equals("MERGE");
        }

        static String status(int code, String state, String msgid, int count) {
            return "sqlcode " + code + " sqlstate " + state + " msgid " + msgid
                + (count >= 0 ? " count " + count : "");
        }

        static String msgid(SQLException x) {
            String m = x.getMessage();
            if (m != null && m.startsWith("[") && m.indexOf(']') > 1) {
                return m.substring(1, m.indexOf(']'));
            }
            int c = Math.abs(x.getErrorCode());
            return c == 0 ? "-" : String.format("SQL%04d", c);
        }

        static String type(String name, int p, int s) {
            switch (name) {
                case "DECIMAL": case "NUMERIC":
                    return name + "(" + p + "," + s + ")";
                case "CHAR": case "VARCHAR": case "GRAPHIC": case "VARGRAPHIC": case "BINARY":
                case "VARBINARY": case "CLOB": case "BLOB": case "DBCLOB": case "DECFLOAT":
                case "NCHAR": case "NVARCHAR": case "NCLOB":
                    return name + "(" + p + ")";
                case "TIMESTAMP":
                    return s == 6 ? name : name + "(" + s + ")";
                default:
                    return name;
            }
        }

        static String value(ResultSet r, int i, int t) throws SQLException {
            switch (t) {
                case Types.DOUBLE: case Types.FLOAT: {
                    double d = r.getDouble(i);
                    return r.wasNull() ? "NULL" : Double.toString(d);
                }
                case Types.REAL: {
                    float f = r.getFloat(i);
                    return r.wasNull() ? "NULL" : Float.toString(f);
                }
                case Types.BINARY: case Types.VARBINARY: case Types.BLOB: case Types.LONGVARBINARY: {
                    byte[] b = r.getBytes(i);
                    if (b == null) {
                        return "NULL";
                    }
                    StringBuilder h = new StringBuilder("X'");
                    for (byte x : b) {
                        h.append(String.format("%02X", x & 0xff));
                    }
                    return h.append('\'').toString();
                }
                case Types.TIMESTAMP: {
                    String v = r.getString(i);
                    return v == null ? "NULL" : v.replaceFirst(" ", "-").replace(':', '.');
                }
                default: {
                    String v = r.getString(i);
                    if (v == null) {
                        return "NULL";
                    }
                    if (t == Types.CHAR || t == Types.VARCHAR || t == Types.CLOB || t == Types.NCHAR
                        || t == Types.NVARCHAR || t == Types.NCLOB || t == Types.LONGVARCHAR
                        || t == Types.DATE || t == Types.TIME || t == Types.OTHER) {
                        return "'" + v.replace("'", "''") + "'";
                    }
                    return v;
                }
            }
        }
    }
}
