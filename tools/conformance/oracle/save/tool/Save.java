// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;

import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.sql.Connection;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.Statement;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Save/restore fact capture over JDBC alone (CL through QSYS.QCMDEXC).
 *   Save ENVFILE oracle|pub400 OUTDIR SCRIPT
 * Script lines:
 *   case NAME TITLE     start OUTDIR/NAME.txt (appends if it exists)
 *   note TEXT           a comment line in the case
 *   boot                make the work programs ZZSPLF, ZZJLOG and files ZZSRC, ZZSPL
 *   cl CMD              run CMD in job 1; the command and its job-log messages go to the case
 *   cl2 CMD             the same in job 2 (a second connection)
 *   qcl CMD             run CMD in job 1, record only failures
 *   sql TITLE           SQL lines up to ";;": a query, rows to the case
 *   sqlx / qsqlx        SQL lines up to ";;": a statement (qsqlx: recorded on failure)
 *   splf TITLE CMD      run CMD, copy the spooled file it made into the case, delete it
 *   qsh TITLE CMD       QSH CMD, its output into the case
 *   src LIB FILE MBR TYPE   source lines up to ";;" into member MBR (made if missing)
 *   joblog on|off       read the job log after each cl (default on)
 *   jobs                the two jobs' names
 * {work} is the work library, {home} the home directory.
 */
public class Save {
    static Connection db, db2;
    static String work, home, ccsid, target;
    static Path out;
    static PrintWriter cur;
    static List<Pattern> masks = new ArrayList<>();
    static List<String> withs = new ArrayList<>();
    static boolean jlog = true;
    static Set<String> seen = new HashSet<>();

    public static void main(String[] a) throws Exception {
        java.util.Map<String, String> env = new java.util.HashMap<>();
        for (String l : Files.readAllLines(Path.of(a[0]))) {
            l = l.strip();
            int e = l.indexOf('=');
            if (e > 0) env.put(l.substring(0, e).strip(), l.substring(e + 1).strip());
        }
        target = a[1];
        String host, user, pass;
        if (target.equals("oracle")) {
            host = env.get("QS_ORACLE_HOST");
            user = env.get("QS_ORACLE_USER");
            pass = env.get("QS_ORACLE_PASS");
            work = "QSTEST";
            home = "/home/QSTEST";
            ccsid = "37";
        } else {
            host = env.get("QS_PUB400_HOST");
            int c = host.indexOf(':');
            if (c > 0) host = host.substring(0, c);
            user = env.get("QS_PUB400_USER");
            pass = env.get("QS_PUB400_PASS");
            work = user.toUpperCase() + "B";
            home = "/home/" + user.toUpperCase();
            ccsid = "273";
        }
        if (System.getenv("QS_WORK") != null) work = System.getenv("QS_WORK");
        if (!user.equalsIgnoreCase("QSECOFR")) addMask(user, "{user}");
        out = Path.of(a[2]);
        Files.createDirectories(out);
        java.util.Properties pr = new java.util.Properties();
        pr.setProperty("naming", "sql");
        pr.setProperty("prompt", "false");
        pr.setProperty("errors", "full");
        com.ibm.as400.access.AS400JDBCDriver drv = new com.ibm.as400.access.AS400JDBCDriver();
        db = drv.connect(mk(host, user, pass), pr, null, false);
        db2 = drv.connect(mk(host, user, pass), pr, null, false);
        try (Statement s = db.createStatement(); ResultSet r = s.executeQuery("SELECT CURRENT SERVER FROM SYSIBM.SYSDUMMY1")) {
            if (r.next()) {
                String n = r.getString(1).strip();
                if (target.equals("oracle") && n.length() >= 3) addMask(n, "{host}");
            }
        }
        exec(db, "CHGJOB INQMSGRPY(*DFT)");
        exec(db2, "CHGJOB INQMSGRPY(*DFT)");
        List<String> lines = Files.readAllLines(Path.of(a[3]));
        for (int i = 0; i < lines.size(); i++) {
            String l = sub(lines.get(i).strip());
            if (l.isEmpty() || l.startsWith("#")) continue;
            String[] p = l.split("\\s+", 3);
            try {
                switch (p[0]) {
                    case "case": {
                        if (cur != null) cur.close();
                        Path f = out.resolve(p[1] + ".txt");
                        boolean exists = Files.exists(f);
                        cur = new PrintWriter(Files.newBufferedWriter(f, StandardCharsets.UTF_8,
                                StandardOpenOption.CREATE, StandardOpenOption.APPEND), true);
                        if (!exists) {
                            w("# " + (p.length > 2 ? p[2] : p[1]));
                            w("# " + (target.equals("oracle") ? "primary oracle (V7R1M0)" : "pub400") + ", " + java.time.LocalDate.now());
                        }
                        System.out.println("== " + p[1]);
                        break;
                    }
                    case "note":
                        w("# " + l.substring(5));
                        break;
                    case "boot":
                        boot();
                        break;
                    case "cl":
                        xcl(db, l.substring(3), true, "");
                        break;
                    case "cl2":
                        xcl(db2, l.substring(4), true, "[job 2] ");
                        break;
                    case "qcl":
                        xcl(db, l.substring(4), false, "");
                        break;
                    case "joblog":
                        jlog = p[1].equals("on");
                        break;
                    case "sql": {
                        StringBuilder q = new StringBuilder();
                        while (!lines.get(++i).strip().equals(";;")) q.append(sub(lines.get(i))).append('\n');
                        sql(l.substring(4), q.toString());
                        break;
                    }
                    case "sqlx":
                    case "qsqlx": {
                        StringBuilder q = new StringBuilder();
                        while (!lines.get(++i).strip().equals(";;")) q.append(sub(lines.get(i))).append('\n');
                        sqlx(q.toString(), p[0].equals("sqlx"));
                        break;
                    }
                    case "dltsplf": {
                        StringBuilder q = new StringBuilder();
                        while (!lines.get(++i).strip().equals(";;")) q.append(sub(lines.get(i))).append(' ');
                        List<String> cmds = new ArrayList<>();
                        try (Statement st = db.createStatement(); ResultSet r = st.executeQuery(q.toString())) {
                            while (r.next()) cmds.add("DLTSPLF FILE(" + r.getString(1).strip() + ") JOB(" + r.getString(2).strip() + ") SPLNBR(" + r.getString(3).strip() + ")");
                        }
                        for (String c : cmds) xcl(db, c, true, "");
                        break;
                    }
                    case "splf":
                        splf(p[1], p[2]);
                        break;
                    case "qsh":
                        qsh(p[1], p[2]);
                        break;
                    case "src": {
                        String[] s = l.split("\\s+");
                        List<String> body = new ArrayList<>();
                        while (!lines.get(++i).equals(";;")) body.add(sub(lines.get(i)));
                        src(s[1], s[2], s[3], s[4], body);
                        break;
                    }
                    case "mark": {
                        try (Statement st = db.createStatement(); ResultSet r = st.executeQuery("SELECT CHAR(CURRENT DATE, ISO), CHAR(CURRENT TIME, ISO) FROM SYSIBM.SYSDUMMY1")) {
                            r.next();
                            String d = r.getString(1), t = r.getString(2);
                            vars.put(p[1] + "_date", d.substring(5, 7) + d.substring(8, 10) + d.substring(2, 4));
                            vars.put(p[1] + "_time", t.replace(".", "").replace(":", ""));
                            w("# mark " + p[1] + ": " + d + " " + t);
                        }
                        break;
                    }
                    case "sleep":
                        Thread.sleep(Long.parseLong(p[1]) * 1000);
                        break;
                    case "jobs":
                        joblog(db);
                        joblog(db2);
                        break;
                    default:
                        throw new IllegalArgumentException("bad line " + l);
                }
            } catch (Exception e) {
                if (System.getenv("QS_TRACE") != null) e.printStackTrace();
                String m = "FAILED " + mask(l) + ": " + mask(String.valueOf(e));
                System.out.println(m);
                if (cur != null) w("# " + m);
            }
        }
        if (cur != null) cur.close();
        db.close();
        db2.close();
    }

    static AS400 mk(String host, String user, String pass) throws Exception {
        AS400 s = new AS400(host, user, pass.toCharArray());
        s.setGuiAvailable(false);
        s.setServicePortsToDefault();
        if (System.getenv("QS_SKIP_SIGNON") != null) {
            java.lang.reflect.Field f = AS400.class.getDeclaredField("skipSignonServer_");
            f.setAccessible(true);
            f.setBoolean(s, true);
            java.lang.reflect.Method choose = AS400.class.getDeclaredMethod("chooseImpl");
            choose.setAccessible(true);
            choose.invoke(s);
            java.lang.reflect.Method get = AS400.class.getDeclaredMethod("getImpl");
            get.setAccessible(true);
            Object impl = get.invoke(s);
            java.lang.reflect.Field t = impl.getClass().getDeclaredField("isPasswordTypeSet_");
            t.setAccessible(true);
            t.setBoolean(impl, true);
        }
        return s;
    }

    static java.util.Map<String, String> vars = new java.util.HashMap<>();

    static String sub(String s) {
        for (var e : vars.entrySet()) s = s.replace("{" + e.getKey() + "}", e.getValue());
        return s.replace("{work}", work).replace("{home}", home);
    }

    static void addMask(String v, String with) {
        masks.add(Pattern.compile(Pattern.quote(v), Pattern.CASE_INSENSITIVE));
        withs.add(Matcher.quoteReplacement(with));
    }

    static String mask(String s) {
        for (int i = 0; i < masks.size(); i++) s = masks.get(i).matcher(s).replaceAll(withs.get(i));
        return s;
    }

    static void w(String s) {
        if (cur == null) {
            System.out.println(mask(s));
            return;
        }
        cur.println(mask(s));
    }

    static String jobname(Connection c) {
        try (Statement s = c.createStatement(); ResultSet r = s.executeQuery("SELECT JOB_NAME FROM TABLE(QSYS2.GET_JOB_INFO('*')) X")) {
            return r.next() ? r.getString(1) : "?";
        } catch (Exception e) {
            return "? " + e.getMessage();
        }
    }

    /** Runs CMD through QCMDEXC; returns the escape message's first level or null. */
    static String exec(Connection c, String cmd) {
        String lit = cmd.replace("'", "''");
        try (Statement cs = c.createStatement()) {
            cs.execute("CALL QSYS.QCMDEXC('" + lit + "', " + String.format("%010d.00000", cmd.length()) + ")");
            return null;
        } catch (java.sql.SQLException e) {
            String m = e.getMessage();
            int k = m.indexOf(" Cause . . .");
            if (k < 0) k = m.indexOf(" Recovery  . . .");
            return k > 0 ? m.substring(0, k) : m;
        }
    }

    static boolean xcl(Connection c, String cmd, boolean record, String who) throws Exception {
        String err = exec(c, cmd);
        List<String> msgs = joblog(c);
        if (!record && err == null) return true;
        w("");
        w(who + "> " + cmd);
        w(who + (err == null ? "  -> ok" : "  -> FAILED " + err));
        for (String m : msgs) w(who + "  " + m);
        System.out.println((err == null ? "ok   " : "FAIL ") + mask(cmd));
        return err == null;
    }

    static final Pattern HDR = Pattern.compile(
        "^(\\*NONE|[A-Z]{2}[A-Z0-9][0-9A-F]{4})\\s+(Completion|Diagnostic|Information|Escape|Inquiry|Request|Reply|Notify|Sender copy|Status)\\s+(\\d\\d)\\s+(\\S+)\\s+(\\S+)\\s+(.*)$");
    static final Pattern NOISE = Pattern.compile("(CPD4090|CPD0912|CPF1301|CPD1672|CPIAD02|CPF1124|CPF2523|SQL0443|CPF1164)\s");
    static final String[] SKIP = {"Cause . . . . . :", "Recovery  . . . :", "Technical description", "From module", "From procedure",
        "Statement", "To module", "To procedure", "From user", "5770", "Job name", "Job description", "MSGID", "Possible choices"};

    /** The job log messages not seen before, as "ID TYPE SEV FROM->TO | text". */
    static List<String> joblog(Connection c) throws Exception {
        List<String> res = new ArrayList<>();
        if (!jlog) return res;
        String e = exec(c, "CALL PGM(" + work + "/ZZJLOG)");
        if (e != null) {
            res.add("# job log not read: " + e);
            return res;
        }
        String hdr = null;
        StringBuilder text = new StringBuilder();
        boolean inMsg = false;
        List<String[]> all = new ArrayList<>();
        for (String l : readSpl(c)) {
            String t = l.strip();
            Matcher m = HDR.matcher(t);
            if (m.matches()) {
                if (hdr != null) all.add(new String[] {hdr, text.toString().strip()});
                hdr = t;
                text.setLength(0);
                inMsg = false;
                continue;
            }
            if (hdr == null) continue;
            if (t.startsWith("Message . . . . :")) {
                inMsg = true;
                text.append(t.substring(17).strip());
                continue;
            }
            boolean skip = false;
            for (String k : SKIP) if (t.startsWith(k)) skip = true;
            if (skip) {
                inMsg = false;
                continue;
            }
            if (inMsg && !t.isEmpty()) text.append(' ').append(t);
        }
        if (hdr != null) all.add(new String[] {hdr, text.toString().strip()});
        for (String[] m : all) {
            if (!seen.add(m[0])) continue;
            if (NOISE.matcher(m[0]).lookingAt()) continue;
            Matcher h = HDR.matcher(m[0]);
            h.matches();
            String rest = h.group(6);
            String body = m[1];
            if (body.contains("ZZJLOG") || body.contains("ZZSPLF") || body.contains("ZZSPL ") || body.contains("ZZSPL.")
                || rest.contains("ZZJLOG") || rest.contains("ZZSPLF") || body.startsWith("-CALL PGM(") || body.contains("QPJOBLOG")) continue;
            String[] r = rest.trim().split("\\s+");
            String from = r.length > 0 ? r[0] : "", to = r.length > 3 ? r[3] : "";
            res.add(h.group(1) + " " + h.group(2) + " " + h.group(3) + " " + from + "->" + to + " | " + body);
        }
        return res;
    }

    static List<String> readSpl(Connection c) throws Exception {
        List<String> b = new ArrayList<>();
        try (Statement s = c.createStatement();
             ResultSet r = s.executeQuery("SELECT CAST(ZZSPL AS VARCHAR(1000) CCSID " + ccsid + ") FROM " + work + ".ZZSPL T ORDER BY RRN(T)")) {
            while (r.next()) b.add(r.getString(1).replaceAll("\\s+$", ""));
        }
        return b;
    }

    static void sql(String title, String q) throws Exception {
        w("");
        w("-- " + title + ": " + q.strip().replace('\n', ' ').replaceAll("\\s+", " "));
        int rows = 0;
        try (Statement s = db.createStatement(); ResultSet r = s.executeQuery(q)) {
            ResultSetMetaData m = r.getMetaData();
            int n = m.getColumnCount();
            StringBuilder h = new StringBuilder();
            for (int c = 1; c <= n; c++) h.append(c > 1 ? "|" : "").append(m.getColumnName(c));
            w(h.toString());
            while (r.next()) {
                StringBuilder b = new StringBuilder();
                for (int c = 1; c <= n; c++) {
                    String v = r.getString(c);
                    b.append(c > 1 ? "|" : "").append(v == null ? "-" : v.strip().replace('\n', ' ').replace('\r', ' '));
                }
                w(b.toString());
                rows++;
            }
            w("(" + rows + " rows)");
        } catch (Exception e) {
            String m = String.valueOf(e.getMessage());
            int k = m.indexOf(" Cause . . .");
            w("# sql error: " + (k > 0 ? m.substring(0, k) : m));
        }
        for (String m : joblog(db)) w("  " + m);
        System.out.println("sql " + title + ": " + rows);
    }

    static void sqlx(String q, boolean record) throws Exception {
        try (Statement s = db.createStatement()) {
            int n = s.executeUpdate(q);
            if (record) {
                w("");
                w("-- " + q.strip().replace('\n', ' ').replaceAll("\\s+", " "));
                w("   -> " + n);
            }
            for (String m : joblog(db)) w("     " + m);
            if (false) {
            }
        } catch (Exception e) {
            String m = String.valueOf(e.getMessage());
            int k = m.indexOf(" Cause . . .");
            w("");
            w("-- " + q.strip().replace('\n', ' ').replaceAll("\\s+", " "));
            w("# sql error: " + (k > 0 ? m.substring(0, k) : m));
            System.out.println("sqlx error: " + mask(m));
        }
    }

    static void src(String lib, String file, String mbr, String type, List<String> body) throws Exception {
        exec(db, "ADDPFM FILE(" + lib + "/" + file + ") MBR(" + mbr + ") SRCTYPE(" + type + ")");
        exec(db, "CLRPFM FILE(" + lib + "/" + file + ") MBR(" + mbr + ")");
        try (Statement s = db.createStatement()) {
            s.executeUpdate("CREATE ALIAS " + lib + ".ZZALIAS FOR " + lib + "." + file + "(" + mbr + ")");
            try (java.sql.PreparedStatement p = db.prepareStatement("INSERT INTO " + lib + ".ZZALIAS (SRCSEQ, SRCDAT, SRCDTA) VALUES(?, 0, ?)")) {
                int n = 1;
                for (String l : body) {
                    p.setBigDecimal(1, new java.math.BigDecimal(n++));
                    p.setString(2, l);
                    p.executeUpdate();
                }
            } finally {
                s.executeUpdate("DROP ALIAS " + lib + ".ZZALIAS");
            }
        }
        System.out.println("src " + file + "/" + mbr + ": " + body.size() + " lines");
    }

    static void boot() throws Exception {
        exec(db, "CRTSRCPF FILE(" + work + "/ZZSRC) RCDLEN(112)");
        exec(db, "CRTPF FILE(" + work + "/ZZSPL) RCDLEN(1000)");
        src(work, "ZZSRC", "ZZSPLF", "CLP", List.of(
            "PGM",
            "DCL VAR(&RCV) TYPE(*CHAR) LEN(70)",
            "DCL VAR(&LEN) TYPE(*CHAR) LEN(4) VALUE(X'00000046')",
            "DCL VAR(&ERR) TYPE(*CHAR) LEN(8) VALUE(X'0000000000000000')",
            "DCL VAR(&FMT) TYPE(*CHAR) LEN(8) VALUE('SPRL0100')",
            "DCL VAR(&FILE) TYPE(*CHAR) LEN(10)",
            "DCL VAR(&JOB) TYPE(*CHAR) LEN(10)",
            "DCL VAR(&USR) TYPE(*CHAR) LEN(10)",
            "DCL VAR(&NBR) TYPE(*CHAR) LEN(6)",
            "DCL VAR(&SPLN) TYPE(*DEC) LEN(7 0)",
            "CALL PGM(QSPRILSP) PARM(&RCV &LEN &FMT &ERR)",
            "CHGVAR VAR(&FILE) VALUE(%SST(&RCV 9 10))",
            "CHGVAR VAR(&JOB) VALUE(%SST(&RCV 19 10))",
            "CHGVAR VAR(&USR) VALUE(%SST(&RCV 29 10))",
            "CHGVAR VAR(&NBR) VALUE(%SST(&RCV 39 6))",
            "CHGVAR VAR(&SPLN) VALUE(%BIN(&RCV 45 4))",
            "CLRPFM FILE(" + work + "/ZZSPL)",
            "CPYSPLF FILE(&FILE) TOFILE(" + work + "/ZZSPL) JOB(&NBR/&USR/&JOB) +",
            "  SPLNBR(&SPLN)",
            "DLTSPLF FILE(&FILE) JOB(&NBR/&USR/&JOB) SPLNBR(&SPLN)",
            "ENDPGM"));
        src(work, "ZZSRC", "ZZJLOG", "CLP", List.of(
            "PGM",
            "DSPJOBLOG JOB(*) OUTPUT(*PRINT)",
            "CALL PGM(" + work + "/ZZSPLF)",
            "RMVMSG PGMQ(*ALLINACT) CLEAR(*ALL)",
            "MONMSG MSGID(CPF0000)",
            "RMVMSG PGMQ(*EXT) CLEAR(*ALL)",
            "MONMSG MSGID(CPF0000)",
            "ENDPGM"));
        for (String p : new String[] {"ZZSPLF", "ZZJLOG"}) {
            String e = exec(db, "CRTBNDCL PGM(" + work + "/" + p + ") SRCFILE(" + work + "/ZZSRC) SRCMBR(" + p + ") REPLACE(*YES)");
            System.out.println("boot " + p + ": " + (e == null ? "ok" : e));
        }
    }

    static void qsh(String title, String cmd) throws Exception {
        String f = home + "/zz.out";
        String sh = "(" + cmd + ") > " + f + " 2>&1; echo \"exit=$?\" >> " + f;
        exec(db, "QSH CMD('" + sh.replace("'", "''") + "')");
        exec(db, "CLRPFM FILE(" + work + "/ZZSPL)");
        String e = exec(db, "CPYFRMSTMF FROMSTMF('" + f + "') TOMBR('/QSYS.LIB/" + work + ".LIB/ZZSPL.FILE/ZZSPL.MBR') MBROPT(*REPLACE) STMFCODPAG(*STMF)");
        List<String> t = readSpl(db);
        exec(db, "RMVLNK OBJLNK('" + f + "')");
        w("");
        w("$ " + cmd + (e == null ? "" : "  (" + e + ")"));
        for (String x : t) w(x);
        System.out.println("qsh " + title + ": " + t.size());
    }

    static void splf(String title, String cmd) throws Exception {
        String err = exec(db, cmd);
        w("");
        w("> " + cmd);
        w(err == null ? "  -> ok" : "  -> FAILED " + err);
        if (err != null) {
            for (String m : joblog(db)) w("  " + m);
            System.out.println("FAIL " + mask(cmd));
            return;
        }
        String e = exec(db, "CALL PGM(" + work + "/ZZSPLF)");
        if (e != null) {
            w("# spooled file not copied: " + e);
            return;
        }
        List<String> t = readSpl(db);
        if (err == null) for (String m : joblog(db)) w("  " + m);
        w("-- spooled file (" + title + ")");
        for (String x : t) w(x);
        System.out.println("splf " + title + ": " + t.size());
    }
}
