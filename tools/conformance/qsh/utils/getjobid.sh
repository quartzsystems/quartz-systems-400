# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# getjobid: the default and -s forms, missing processes, messages.

probe self 'getjobid'
probe pid 'getjobid $$'
probe short 'getjobid -s $$'
probe read 'getjobid -s $$ | { read n u j; echo "$j/$u/$n"; }'
probe missing 'getjobid 999999; echo $?'
probe usage 'getjobid abc; echo $?; getjobid -x; echo $?'
finish
