// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;

/** Read-only copy of IFS files from the primary. Args: outdir path... */
public final class NsGet {
    public static void main(String[] a) throws Exception {
        AS400 sys = new AS400(System.getenv("QS_ORACLE_HOST"), System.getenv("QS_ORACLE_USER"), System.getenv("QS_ORACLE_PASS"));
        for (int i = 1; i < a.length; i++) {
            IFSFile f = new IFSFile(sys, a[i]);
            try (InputStream in = new IFSFileInputStream(f, IFSFileInputStream.SHARE_ALL); OutputStream o = new FileOutputStream(a[0] + "/" + f.getName())) {
                in.transferTo(o);
            }
            System.out.println(a[i] + " ok " + f.length() + " modified=" + new java.util.Date(f.lastModified()));
        }
        sys.disconnectAllServices();
    }
}
