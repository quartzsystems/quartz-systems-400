// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;
import java.nio.charset.Charset;
import java.util.*;

/**
 * Read-only: QZLSOLST and QZLSLSTI on the primary, every list format, and an invalid format.
 * Only QTEMP is written (the user space). Arg: outdir. Errors come back as ERRC0100 (no text).
 */
public final class NsList {
    static final AS400Bin4 BIN4 = new AS400Bin4();
    static final Charset E = Charset.forName("Cp037");
    static AS400 s;
    static String host, user;

    static final String[] FORMATS = {"ZLSL0100", "ZLSL0101", "ZLSL0200", "ZLSL0201", "ZLSL0300", "ZLSL0400", "ZLSL0600", "ZLSL0700", "ZLSL0800", "ZLSL0900"};

    static byte[] e(String v, int n) {
        byte[] b = new byte[n];
        Arrays.fill(b, (byte) 0x40);
        byte[] x = v.getBytes(E);
        System.arraycopy(x, 0, b, 0, Math.min(n, x.length));
        return b;
    }

    static byte[] errc() {
        byte[] b = new byte[272];
        BIN4.toBytes(272, b, 0);
        return b;
    }

    static String mask(String t) {
        if (host != null && !host.isEmpty()) t = t.replace(host, "{host}");
        if (user != null && !user.isEmpty() && !user.equalsIgnoreCase("QSECOFR")) t = t.replaceAll("(?i)" + java.util.regex.Pattern.quote(user), "{user}");
        return t;
    }

    static String err(byte[] ec) {
        int avail = BIN4.toInt(ec, 4);
        if (avail == 0) return "none";
        String id = new String(ec, 8, 7, E);
        int n = Math.max(0, Math.min(avail, 272) - 16);
        StringBuilder h = new StringBuilder();
        for (int i = 0; i < n; i++) h.append(String.format("%02X", ec[16 + i]));
        return id + " available=" + avail + " data=" + h;
    }

    static void hex(PrintWriter w, byte[] b, int len) {
        boolean starred = false;
        for (int i = 0; i < len; i += 16) {
            if (i > 0 && i + 16 <= len && Arrays.equals(b, i, i + 16, b, i - 16, i)) {
                if (!starred) w.println("*");
                starred = true;
                continue;
            }
            starred = false;
            StringBuilder h = new StringBuilder(String.format("%06X ", i));
            StringBuilder t = new StringBuilder();
            for (int j = i; j < i + 16; j++) {
                if (j < len) {
                    h.append(String.format("%02X", b[j]));
                    char c = new String(b, j, 1, E).charAt(0);
                    t.append(c >= 0x20 && c < 0x7f ? c : '.');
                } else h.append("  ");
                if (j % 4 == 3) h.append(' ');
            }
            w.println(mask(h + " |" + t + "|"));
        }
    }

    // Field tables: name, kind (i=int4, c=char n, l=int8, x=hex n), length.
    static final Map<String, String[]> FIELDS = new HashMap<>();
    static {
        String common = "i:size,c12:shareName,i:deviceType,i:permissions,i:maxUsers,i:currentUsers,i:spoolFileType,i:pathOffset,i:pathLength,c20:outq,c50:printDriver,c50:text";
        FIELDS.put("ZLSL0100", common.split(","));
        FIELDS.put("ZLSL0101", (common + ",c20:printerFile,i:textCcsid,i:extOffset,i:extCount,x1:textConv,x1:publish,x1:encrypt,c10:autl").split(","));
        String a = "i:codePage,i:pCodePage,i:idle,i:pIdle,i:oplock,i:pOplock,i:browse,i:pBrowse,i:winsProxy,i:pWinsProxy,i:guestSupport,i:pGuestSupport,c10:guest,c10:pGuest,c15:server,c15:pServer,c15:domain,c15:pDomain,c50:text,c50:pText,c15:winsPri,c15:pWinsPri,c15:winsSec,c15:pWinsSec,c224:scope,c224:pScope";
        FIELDS.put("ZLSL0200", a.split(","));
        FIELDS.put("ZLSL0201", (a + ",x1:sysNameAccess,x1:pSysNameAccess,x1:authMethod,x1:pAuthMethod,i:role,i:pRole,i:msgAuth,i:pMsgAuth,i:minSev,i:pMinSev,i:lanman,i:pLanman,i:encrypt,i:pEncrypt,c10:autl,c10:pAutl").split(","));
        FIELDS.put("ZLSL0300", "c15:workstation,c10:user,x3:r1,i:connections,i:opens,i:sessions,i:time,i:idle,x1:logonType,x1:encPwd,x6:r2,l:sessionId,i:wsNameType,c124:expandedWs".split(","));
        FIELDS.put("ZLSL0400", "i:opens,i:printJobs,i:sessionStarts,i:autoDisc,i:regDisc,i:dupSessions,i:sessionErrors,i:pwdViolations,i:guestSupport,i:unknownUser,i:sentLow,i:sentHigh,i:rcvLow,i:rcvHigh,i:avgResp,c13:startTime,c13:resetTime,x10:reserved".split(","));
        FIELDS.put("ZLSL0600", "i:connId,i:connType,i:filesOpen,i:users,i:time,c10:user,c12:share,x6:r,l:sessionId,x8:r2".split(","));
        FIELDS.put("ZLSL0700", "i:connId,i:connType,i:filesOpen,i:users,i:time,c10:user,c15:workstation,x3:r,l:sessionId,x8:r2,i:wsNameType,c124:expandedWs".split(","));
        FIELDS.put("ZLSL0800", "c12:shareName,i:deviceType".split(","));
        FIELDS.put("ZLSL0900", "c10:user".split(","));
    }

    static boolean relative;

    static void decode(PrintWriter w, String fmt, byte[] b, int off, int limit) {
        int p = off;
        for (String f : FIELDS.get(fmt)) {
            String[] k = f.split(":");
            String kind = k[0];
            String v;
            int n;
            if (kind.equals("i")) { n = 4; v = p + n <= limit ? Integer.toString(BIN4.toInt(b, p)) : "?"; }
            else if (kind.equals("l")) { n = 8; v = p + n <= limit ? Long.toString(new AS400Bin8().toLong(b, p)) : "?"; }
            else {
                n = Integer.parseInt(kind.substring(1));
                if (p + n > limit) v = "?";
                else if (kind.charAt(0) == 'c') v = "'" + new String(b, p, n, E) + "'";
                else { StringBuilder h = new StringBuilder("x'"); for (int i = 0; i < n; i++) h.append(String.format("%02X", b[p + i])); v = h + "'"; }
            }
            w.println(mask(String.format("  +%-4d %-16s %s", p - off, k[1], v)));
            p += n;
        }
        if (fmt.equals("ZLSL0100") || fmt.equals("ZLSL0101")) {
            int po = BIN4.toInt(b, off + 36), pl = BIN4.toInt(b, off + 40);
            // ZLSL0100 from QZLSOLST counts from the entry; everything else from the buffer's start.
            int base = relative && fmt.equals("ZLSL0100") ? off : 0;
            if (po > 0 && base + po + pl <= b.length) {
                w.println(mask("  path (job CCSID at " + (base == 0 ? "" : "+") + po + ", " + pl + " bytes): '" + new String(b, base + po, pl, E) + "'"));
            }
            if (fmt.equals("ZLSL0101")) {
                int eo = BIN4.toInt(b, off + 188), en = BIN4.toInt(b, off + 192);
                for (int i = 0; i < en && eo + 50 * (i + 1) <= b.length; i++) {
                    int q = eo + 50 * i;
                    int el = BIN4.toInt(b, q);
                    w.println("  extension " + i + " len=" + el + " '" + new String(b, q + 4, Math.max(0, Math.min(el, 46)), E) + "'");
                }
            }
        }
    }

    static void olst(String dir, String fmt, String qual) throws Exception {
        relative = true;
        int rlen = 65535;
        byte[] ec = errc();
        ProgramParameter[] p = {
            new ProgramParameter(rlen),
            new ProgramParameter(BIN4.toBytes(rlen)),
            new ProgramParameter(64),
            new ProgramParameter(e(fmt, 8)),
            new ProgramParameter(e(qual, 15)),
            new ProgramParameter(ec, 272),
        };
        ProgramCall c = new ProgramCall(s, "/QSYS.LIB/QZLSOLST.PGM", p);
        boolean ok = c.run();
        byte[] ecOut = p[5].getOutputData();
        String tag = qual.isEmpty() ? "blank" : qual.replace("*", "");
        try (PrintWriter w = new PrintWriter(new FileWriter(dir + "/olst-" + fmt + "-" + tag + ".txt"))) {
            w.println("# QZLSOLST receiver=65535 format=" + fmt + " qualifier='" + qual + "' on the primary (V7R1M0), 2026-11-09");
            w.println("result=" + ok + " error=" + err(ecOut));
            for (AS400Message m : c.getMessageList()) w.println("message " + m.getID() + " sev=" + m.getSeverity());
            if (!ok || BIN4.toInt(ecOut, 4) != 0) { System.out.println("olst " + fmt + " " + tag + " " + err(ecOut)); return; }
            byte[] li = p[2].getOutputData();
            int total = BIN4.toInt(li, 0), ret = BIN4.toInt(li, 4), rl = BIN4.toInt(li, 8), info = BIN4.toInt(li, 12);
            w.println("list info: total=" + total + " returned=" + ret + " recordLength=" + rl + " lengthOfInfo=" + info
                + " complete='" + new String(li, 16, 1, E) + "' created='" + new String(li, 17, 13, E) + "'");
            w.println("list info hex:");
            hex(w, li, 64);
            byte[] r = p[0].getOutputData();
            int used = Math.min(rlen, Math.max(info, ret * Math.max(rl, 0)));
            w.println("receiver hex (" + used + " bytes):");
            hex(w, r, used);
            w.println("decoded:");
            int off = 0;
            for (int i = 0; i < ret; i++) {
                int size = (fmt.equals("ZLSL0100") || fmt.equals("ZLSL0101")) ? BIN4.toInt(r, off) : rl;
                w.println(" entry " + i + " at " + off + " size " + size);
                decode(w, fmt, r, off, Math.min(used, off + Math.max(size, 1)));
                off += size <= 0 ? rl : size;
            }
            System.out.println("olst " + fmt + " " + tag + " total=" + total + " ret=" + ret + " rl=" + rl + " info=" + info);
        }
    }

    static boolean call(String pgm, ProgramParameter[] p) throws Exception {
        ProgramCall c = new ProgramCall(s, pgm, p);
        boolean ok = c.run();
        if (!ok) for (AS400Message m : c.getMessageList()) System.out.println("  " + pgm + " " + m.getID());
        return ok;
    }

    static void lsti(String dir, String fmt, String qual) throws Exception {
        relative = false;
        byte[] ec = errc();
        ProgramParameter[] p = {
            new ProgramParameter(e("ZLSSPC    QTEMP", 20)),
            new ProgramParameter(e(fmt, 8)),
            new ProgramParameter(e(qual, 15)),
            new ProgramParameter(ec, 272),
        };
        ProgramCall c = new ProgramCall(s, "/QSYS.LIB/QZLSLSTI.PGM", p);
        boolean ok = c.run();
        byte[] ecOut = p[3].getOutputData();
        String tag = qual.isEmpty() ? "blank" : qual.replace("*", "");
        try (PrintWriter w = new PrintWriter(new FileWriter(dir + "/lsti-" + fmt + "-" + tag + ".txt"))) {
            w.println("# QZLSLSTI user space QTEMP/ZLSSPC format=" + fmt + " qualifier='" + qual + "' on the primary (V7R1M0), 2026-11-09");
            w.println("result=" + ok + " error=" + err(ecOut));
            for (AS400Message m : c.getMessageList()) w.println("message " + m.getID() + " sev=" + m.getSeverity());
            if (!ok || BIN4.toInt(ecOut, 4) != 0) { System.out.println("lsti " + fmt + " " + tag + " " + err(ecOut)); return; }
            byte[] hdr = rtvus(1, 192);
            int lo = BIN4.toInt(hdr, 124), ls = BIN4.toInt(hdr, 128), n = BIN4.toInt(hdr, 132), es = BIN4.toInt(hdr, 136);
            int io = BIN4.toInt(hdr, 108), is = BIN4.toInt(hdr, 112), ho = BIN4.toInt(hdr, 116), hs = BIN4.toInt(hdr, 120);
            int gs = BIN4.toInt(hdr, 0x40), used = BIN4.toInt(hdr, 0x68);
            w.println("generic header: size=" + gs + " level='" + new String(hdr, 0x44, 4, E) + "' format='" + new String(hdr, 0x48, 8, E) + "' api='" + new String(hdr, 0x50, 10, E)
                + "' created='" + new String(hdr, 0x5A, 13, E) + "' status='" + new String(hdr, 0x67, 1, E) + "' used=" + used + " inputOffset=" + io + " inputSize=" + is + " headerOffset=" + ho + " headerSize=" + hs
                + " listOffset=" + lo + " listSize=" + ls + " entries=" + n + " entrySize=" + es + " ccsid=" + BIN4.toInt(hdr, 0x8C));
            int end = Math.max(used, Math.max(lo + ls, io + is));
            byte[] all = rtvus(1, Math.min(end, 65536));
            w.println("user space hex (" + all.length + " bytes):");
            hex(w, all, all.length);
            w.println("decoded:");
            int off = lo;
            for (int i = 0; i < n; i++) {
                if (off + 4 > all.length) { w.println(" (entry " + i + " beyond the list data)"); break; }
                int size = (fmt.equals("ZLSL0100") || fmt.equals("ZLSL0101")) ? BIN4.toInt(all, off) : es;
                w.println(" entry " + i + " at " + off + " size " + size);
                decode(w, fmt, all, off, Math.min(all.length, off + Math.max(size, 1)));
                off += size <= 0 ? es : size;
            }
            System.out.println("lsti " + fmt + " " + tag + " entries=" + n + " es=" + es);
        }
    }

    static byte[] rtvus(int start, int len) throws Exception {
        ProgramParameter[] p = {
            new ProgramParameter(e("ZLSSPC    QTEMP", 20)),
            new ProgramParameter(BIN4.toBytes(start)),
            new ProgramParameter(BIN4.toBytes(len)),
            new ProgramParameter(len),
        };
        if (!call("/QSYS.LIB/QUSRTVUS.PGM", p)) throw new IOException("QUSRTVUS");
        return p[3].getOutputData();
    }

    public static void main(String[] a) throws Exception {
        String dir = a[0];
        host = System.getenv("QS_ORACLE_HOST");
        user = System.getenv("QS_ORACLE_USER");
        s = new AS400(host, user, System.getenv("QS_ORACLE_PASS"));
        s.setGuiAvailable(false);
        for (String f : FORMATS) {
            olst(dir, f, "*ALL");
            olst(dir, f, "");
        }
        olst(dir, "ZLSL9999", "*ALL");
        olst(dir, "ZLSL0100", "NOSUCHSHARE");
        olst(dir, "ZLSL0100", "QIBM");
        // QTEMP user space for QZLSLSTI.
        ProgramParameter[] cr = {
            new ProgramParameter(e("ZLSSPC    QTEMP", 20)),
            new ProgramParameter(e("NSPROBE", 10)),
            new ProgramParameter(BIN4.toBytes(65536)),
            new ProgramParameter(new byte[] {0}),
            new ProgramParameter(e("*ALL", 10)),
            new ProgramParameter(e("NetServer probe", 50)),
            new ProgramParameter(e("*YES", 10)),
            new ProgramParameter(errc(), 272),
        };
        if (!call("/QSYS.LIB/QUSCRTUS.PGM", cr)) throw new IOException("QUSCRTUS");
        for (String f : FORMATS) {
            lsti(dir, f, "*ALL");
            lsti(dir, f, "");
        }
        lsti(dir, "ZLSL9999", "*ALL");
        s.disconnectAllServices();
    }
}
