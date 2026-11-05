// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

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
import java.util.List;

/**
 * The SQL PL probe: a case file as SqlKit runs it, plus lines that set the
 * connection between statements: "-- autocommit on|off", "-- isolation
 * none|chg|cs|all", and "-- cmd COMMAND" runs a CL command (its message
 * IDs printed). Compiled with SqlKit.java (it uses SqlKit.Case).
 *
 *   java -cp CLASSES:jt400.jar PlProbe primary|pub400 FILE.sql
 *
 * Prints the outcome in the kit's form. Credentials as SqlKit's.
 */
public final class PlProbe {
    public static void main(String[] a) throws Exception {
        java.util.Map<String, String> e = new java.util.HashMap<>(System.getenv());
        Path envf = Path.of(e.getOrDefault("QS_ORACLE_ENV", "tools/conformance/oracle/.env"));
        for (String line : Files.readAllLines(envf, StandardCharsets.UTF_8)) {
            int eq = line.indexOf('=');
            if (eq > 0 && !line.startsWith("#")) {
                e.putIfAbsent(line.substring(0, eq).trim(), line.substring(eq + 1).trim());
            }
        }
        String pre = a[0].equals("pub400") ? "QS_PUB400_" : "QS_ORACLE_";
        String host = e.get(pre + "HOST");
        String user = e.get(pre + "USER");
        String pass = e.get(pre + "PASS");
        String lib = a[0].equals("pub400")
            ? e.getOrDefault("QS_PUB400_LIB", user.toUpperCase() + "1").toUpperCase()
            : "QSTEST";
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + host + ";naming=sql;prompt=false;errors=full"
            + ";date format=iso;time format=iso;decimal separator=.;libraries=" + lib;
        String text = Files.readString(Path.of(a[1]), StandardCharsets.UTF_8);
        StringBuilder sb = new StringBuilder("-- captured from " + a[0] + "\n");
        try (Connection c = DriverManager.getConnection(url, user, pass)) {
            try (Statement s = c.createStatement()) {
                s.execute("SET SCHEMA " + lib);
            }
            StringBuilder chunk = new StringBuilder();
            for (String line : text.split("\r?\n", -1)) {
                String t = line.trim().toLowerCase();
                if (t.startsWith("-- cmd ")) {
                    flush(c, chunk, lib, sb);
                    String cmd = line.trim().substring("-- cmd ".length());
                    com.ibm.as400.access.AS400 sys = new com.ibm.as400.access.AS400(host, user, pass);
                    com.ibm.as400.access.CommandCall cc = new com.ibm.as400.access.CommandCall(sys);
                    boolean ok = cc.run(cmd);
                    sb.append("-- cmd ").append(cmd.replace(lib, "{lib}")).append(ok ? " ok" : " failed");
                    for (com.ibm.as400.access.AS400Message m : cc.getMessageList()) {
                        sb.append(' ').append(m.getID());
                    }
                    sb.append("\n");
                    sys.disconnectAllServices();
                    continue;
                }
                if (t.startsWith("-- autocommit ") || t.startsWith("-- isolation ")) {
                    flush(c, chunk, lib, sb);
                    sb.append(line.trim()).append('\n');
                    if (t.startsWith("-- autocommit ")) {
                        c.setAutoCommit(t.endsWith("on"));
                    } else {
                        String l = t.substring("-- isolation ".length());
                        c.setTransactionIsolation(switch (l) {
                            case "none" -> Connection.TRANSACTION_NONE;
                            case "chg" -> Connection.TRANSACTION_READ_UNCOMMITTED;
                            case "cs" -> Connection.TRANSACTION_READ_COMMITTED;
                            case "all" -> Connection.TRANSACTION_REPEATABLE_READ;
                            default -> Connection.TRANSACTION_SERIALIZABLE;
                        });
                    }
                    continue;
                }
                chunk.append(line).append('\n');
            }
            flush(c, chunk, lib, sb);
        }
        System.out.print(sb.toString().replace(lib, "{lib}").replace(user.toUpperCase(), "{user}"));
    }

    private static void flush(Connection c, StringBuilder chunk, String lib, StringBuilder sb) {
        List<String> stmts = SqlKit.Case.statements(chunk.toString());
        chunk.setLength(0);
        for (String st : stmts) {
            sb.append("> ").append(SqlKit.Case.first(st)).append('\n');
            try (Statement s = c.createStatement()) {
                boolean rows = s.execute(st.replace("{lib}", lib));
                int count = rows ? -1 : s.getUpdateCount();
                sb.append(SqlKit.Case.status(0, "00000", "-", SqlKit.Case.changes(st) ? count : -1)).append('\n');
                for (SQLWarning w = s.getWarnings(); w != null; w = w.getNextWarning()) {
                    sb.append("warning ").append(w.getErrorCode()).append(' ').append(w.getSQLState())
                        .append(' ').append(SqlKit.Case.msgid(w)).append('\n');
                }
                if (rows) {
                    try (ResultSet r = s.getResultSet()) {
                        ResultSetMetaData m = r.getMetaData();
                        int n = m.getColumnCount();
                        for (int i = 1; i <= n; i++) {
                            sb.append("# ").append(m.getColumnLabel(i)).append(' ')
                                .append(SqlKit.Case.type(m.getColumnTypeName(i), m.getPrecision(i), m.getScale(i)))
                                .append('\n');
                        }
                        while (r.next()) {
                            StringBuilder row = new StringBuilder("|");
                            for (int i = 1; i <= n; i++) {
                                row.append(' ').append(SqlKit.Case.value(r, i, m.getColumnType(i))).append(" |");
                            }
                            sb.append(row).append('\n');
                        }
                    }
                }
            } catch (SQLException x) {
                sb.append(SqlKit.Case.status(x.getErrorCode(), x.getSQLState(), SqlKit.Case.msgid(x), -1)).append('\n');
                String m = x.getMessage();
                if (m != null) {
                    sb.append("  : ").append(m.length() > 110 ? m.substring(0, 110) : m).append('\n');
                }
            }
        }
    }
}
