// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.AS400Message;
import com.ibm.as400.access.CommandCall;

/** Run CL commands and print every message with its second-level text. */
public class Help {
    public static void main(String[] a) throws Exception {
        AS400 sys = new AS400(a[0], a[1], a[2].toCharArray());
        sys.setGuiAvailable(false);
        CommandCall cc = new CommandCall(sys);
        for (int i = 3; i < a.length; i++) {
            boolean ok = cc.run(a[i]);
            System.out.println((ok ? "OK  " : "ERR ") + a[i]);
            for (AS400Message m : cc.getMessageList()) {
                m.load();
                System.out.println("  " + m.getID() + " " + m.getText());
                String h = m.getHelp();
                if (h != null && !h.isBlank()) System.out.println("    " + h.replaceAll("\\s+", " "));
            }
        }
        sys.disconnectAllServices();
    }
}
