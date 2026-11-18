# The message descriptions of a DSPMSGD DETAIL(*FULL) print as one line
# each: ID, severity, data fields, first-level text.
function flush() {
  if (id != "") printf "%s|%s|%s|%s\n", id, sev, fields, msg
  id = ""; sev = ""; fields = ""; msg = ""; inf = 0; inm = 0
}
/^ [A-Z][A-Z][A-Z][0-9A-F][0-9A-F][0-9A-F][0-9A-F]$/ { flush(); id = substr($0, 2); next }
id == "" { next }
/^   Message \. \. \. \. :/ { msg = substr($0, 23); inm = 1; next }
/^   Cause|^   Recovery|^   Severity|^ 5770SS1/ { inm = 0 }
inm && /^     [^ ]/ { m = $0; sub(/^ +/, "", m); msg = msg " " m; next }
/^   Severity/ { sev = $NF; next }
/^     Field +Data Type/ { inf = 1; next }
inf && /^      &[0-9]+ / {
  f = $1 " " $2; if (NF >= 3) f = f " " $3; if (NF >= 4) f = f " " $4
  fields = fields (fields == "" ? "" : "; ") f; next
}
inf && /^   [A-Z]/ { inf = 0 }
END { flush() }
