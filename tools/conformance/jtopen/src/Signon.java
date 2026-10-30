// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.security.auth.ProfileTokenCredential;

/** Sign-on server: validate, sign-on information, a profile token, an unknown user. */
public final class Signon {
    public static void main(String[] a) {
        AS400 s = Hs.signonSystem();
        Hs.step("validate", () -> Hs.out("     valid=" + s.validateSignon()));
        Hs.step("vrm", () -> Hs.out("     vrm=" + Integer.toHexString(s.getVRM())));
        Hs.step("signon info", () -> {
            Hs.out("     expiration days known=" + (s.getPasswordExpirationDate() != null));
            Hs.out("     previous signon known=" + (s.getPreviousSignonDate() != null));
            Hs.out("     warning days=" + s.getSystemPasswordExpirationWarningDays());
        });
        Hs.step("profile token", () -> {
            ProfileTokenCredential t = s.getProfileToken();
            Hs.out("     token bytes=" + t.getToken().length + " type=" + t.getTokenType());
        });
        Hs.step("unknown user", () -> {
            AS400 bad = new AS400(Hs.host(), "QSNOUSR1", "NOPASSWD1");
            bad.setGuiAvailable(false);
            bad.setServicePortsToDefault();
            Hs.ports(bad);
            Hs.out("     valid=" + bad.validateSignon());
        });
        s.disconnectAllServices();
    }
}
