// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.Statement;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * The text of the account's own spooled files on an oracle, oldest
 * first, one file per spooled file (`NN-NAME.txt` in OUTDIR, the
 * account written as {user}); with `--delete` each is deleted after it
 * is read. Used for STRSQL's printed session and query, RUNSQLSTM's
 * listing and PRTSQLINF's report.
 *
 *   java SplfText pub400|primary OUTDIR [--delete] [--last N] [--since TS] [--jobs PREFIX] [NAME...]
 */
public final class SplfText {
    private static Map<String, String> env() throws IOException {
        Map<String, String> m = new HashMap<>(System.getenv());
        Path p = Path.of(m.getOrDefault("QS_ORACLE_ENV", "tools/conformance/oracle/.env"));
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
        Map<String, String> e = env();
        String pre = a[0].equals("pub400") ? "QS_PUB400_" : "QS_ORACLE_";
        String host = e.get(pre + "HOST");
        String user = e.get(pre + "USER");
        String pass = e.get(pre + "PASS");
        Path out = Path.of(a[1]);
        boolean delete = false;
        int last = Integer.MAX_VALUE;
        String since = "0001-01-01-00.00.00";
        String jobs = "";
        List<String> names = new ArrayList<>();
        for (int i = 2; i < a.length; i++) {
            if (a[i].equals("--delete")) {
                delete = true;
            } else if (a[i].equals("--last")) {
                last = Integer.parseInt(a[++i]);
            } else if (a[i].equals("--since")) {
                since = a[++i];
            } else if (a[i].equals("--jobs")) {
                jobs = a[++i].toUpperCase();
            } else {
                names.add(a[i].toUpperCase());
            }
        }
        Files.createDirectories(out);
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + host + ";naming=sql;prompt=false;errors=full";
        try (Connection c = DriverManager.getConnection(url, user, pass)) {
            List<String[]> files = new ArrayList<>();
            String q = "SELECT JOB_NAME, SPOOLED_FILE_NAME, FILE_NUMBER FROM QSYS2.OUTPUT_QUEUE_ENTRIES_BASIC"
                + " WHERE USER_NAME = ? AND CREATE_TIMESTAMP >= CAST(? AS TIMESTAMP) AND JOB_NAME LIKE ?"
                + " ORDER BY CREATE_TIMESTAMP";
            try (PreparedStatement s = c.prepareStatement(q)) {
                s.setString(1, user.toUpperCase());
                s.setString(2, since);
                s.setString(3, "%/" + jobs + "%");
                try (ResultSet r = s.executeQuery()) {
                    while (r.next()) {
                        String n = r.getString(2).trim();
                        if (names.isEmpty() || names.contains(n)) {
                            files.add(new String[] {r.getString(1).trim(), n, r.getString(3)});
                        }
                    }
                }
            }
            if (files.size() > last) {
                files = files.subList(files.size() - last, files.size());
            }
            int k = 0;
            for (String[] f : files) {
                StringBuilder sb = new StringBuilder();
                String d = "SELECT SPOOLED_DATA FROM TABLE(SYSTOOLS.SPOOLED_FILE_DATA(JOB_NAME => ?,"
                    + " SPOOLED_FILE_NAME => ?, SPOOLED_FILE_NUMBER => ?)) ORDER BY ORDINAL_POSITION";
                try (PreparedStatement s = c.prepareStatement(d)) {
                    s.setString(1, f[0]);
                    s.setString(2, f[1]);
                    s.setInt(3, Integer.parseInt(f[2]));
                    try (ResultSet r = s.executeQuery()) {
                        while (r.next()) {
                            String line = r.getString(1);
                            sb.append(line == null ? "" : line.replaceAll("\\s+$", "")).append('\n');
                        }
                    }
                }
                String text = sb.toString().replaceAll("(?i)" + java.util.regex.Pattern.quote(user), "{user}");
                k++;
                Files.writeString(out.resolve(String.format("%02d-%s.txt", k, f[1])), text, StandardCharsets.UTF_8);
                System.out.println(k + " " + f[1] + " " + f[0].replaceAll("(?i)" + java.util.regex.Pattern.quote(user), "{user}"));
                if (delete) {
                    String cmd = "DLTSPLF FILE(" + f[1] + ") JOB(" + f[0] + ") SPLNBR(" + f[2] + ")";
                    try (Statement s = c.createStatement()) {
                        s.execute("CALL QSYS2.QCMDEXC('" + cmd + "')");
                    }
                }
            }
        }
    }
}
