// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.SQLException;
import java.sql.Statement;
import java.util.HashMap;
import java.util.Map;

/**
 * Statements run in order on an oracle (`*SQL` naming, autocommit), a
 * line `@` ending each; `{lib}` is the account's library and `{user}`
 * the account. Prints each statement's SQLCODE. Used to set up and
 * clean up what a listing capture needs (a stream file through
 * `QSYS2.IFS_WRITE`, `RUNSQLSTM` and `PRTSQLINF` through
 * `QSYS2.QCMDEXC`).
 *
 *   java SqlSteps pub400|primary STEPS.txt
 */
public final class SqlSteps {
    public static void main(String[] a) throws Exception {
        Map<String, String> e = new HashMap<>(System.getenv());
        Path envf = Path.of(e.getOrDefault("QS_ORACLE_ENV", "tools/conformance/oracle/.env"));
        if (Files.exists(envf)) {
            for (String line : Files.readAllLines(envf, StandardCharsets.UTF_8)) {
                int eq = line.indexOf('=');
                if (eq > 0 && !line.startsWith("#")) {
                    e.putIfAbsent(line.substring(0, eq).trim(), line.substring(eq + 1).trim());
                }
            }
        }
        String pre = a[0].equals("pub400") ? "QS_PUB400_" : "QS_ORACLE_";
        String user = e.get(pre + "USER");
        String lib = a[0].equals("pub400")
            ? e.getOrDefault("QS_PUB400_LIB", user.toUpperCase() + "1").toUpperCase()
            : "QSTEST";
        String url = "jdbc:as400://" + e.get(pre + "HOST") + ";naming=sql;prompt=false;errors=full";
        String text = Files.readString(Path.of(a[1]), StandardCharsets.UTF_8)
            .replace("{lib}", lib).replace("{user}", user.toUpperCase());
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        try (Connection c = DriverManager.getConnection(url, user, e.get(pre + "PASS"));
             Statement s = c.createStatement()) {
            StringBuilder cur = new StringBuilder();
            for (String line : text.split("\n", -1)) {
                if (!line.trim().equals("@")) {
                    if (!line.startsWith("--")) {
                        cur.append(line).append('\n');
                    }
                    continue;
                }
                String stmt = cur.toString().trim();
                cur.setLength(0);
                if (stmt.isEmpty()) {
                    continue;
                }
                String shown = stmt.replace(lib, "{lib}").replace(user.toUpperCase(), "{user}");
                try {
                    s.execute(stmt);
                    System.out.println("0 " + shown.lines().findFirst().orElse(""));
                } catch (SQLException x) {
                    System.out.println(x.getErrorCode() + " " + shown.lines().findFirst().orElse(""));
                }
            }
        }
    }
}
