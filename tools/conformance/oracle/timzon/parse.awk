# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# Turn the output of probes/rel/timzon71-details.tn (`show` after each
# option 5) into zones.tsv: awk -f parse.awk OUT > zones.tsv
function val(line,  v) { v = substr(line, index(line, ":   ") + 4); sub(/ *\|$/, "", v); sub(/^ +/, "", v); sub(/ +$/, "", v); return v }
function put(k, v) { if (!(k in rec)) rec[k] = v }
function flush() { if (zone != "") { printf "%s", zone; for (i = 1; i <= nk; i++) printf "\t%s", (keys[i] in rec ? rec[keys[i]] : "-"); printf "\n" } delete rec }
BEGIN { OFS="\t"; split("off stdfull stdabbr stdmsg dstfull dstabbr dstmsg msgf msgflib smon sday srel stime emon eday erel etime shift yoff alt text", keys, " "); nk = 21
  printf "zone"; for (i=1;i<=nk;i++) printf "\t%s", keys[i]; printf "\n" }
/^DETAIL / { flush(); zone = $2; sec = ""; next }
/Time zone  \. \./ { next }
/\| Offset \. \./ { put("off", val($0)) }
/\| Standard Time:/ { sec = "std" }
/\| Daylight Saving Time \(DST\):/ { sec = "dst" }
/\| Daylight Saving Time start:/ { sec = "s" }
/\| Daylight Saving Time end:/ { sec = "e" }
/Full name  \. \./ { put(sec "full", val($0)) }
/Abbreviated name \./ { put(sec "abbr", val($0)) }
/  Message  \. \./ { put(sec "msg", val($0)) }
/\| Message file \./ { put("msgf", val($0)) }
/  Library  \. \./ { put("msgflib", val($0)) }
/  Month  \. \./ { put(sec "mon", val($0)) }
/  Day  \. \./ { put(sec "day", val($0)) }
/Relative day of month/ { put(sec "rel", val($0)) }
/  Time \. \. \./ { put(sec "time", val($0)) }
/Daylight Saving Time shift/ { put("shift", val($0)) }
/Year offset  \./ { put("yoff", val($0)) }
/Alternate name \. \./ { put("alt", val($0)) }
/\| Text \. \./ { put("text", val($0)) }
END { flush() }
