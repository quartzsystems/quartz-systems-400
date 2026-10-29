# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# chown: by name and number, owner:group, -R, unknown names, errors.
# Only the probing profile's own files under $T are changed, to itself.

probe self 'chown $LOGNAME data/fruits.txt; ls -l data/fruits.txt'
probe self-lower 'chown $(echo $LOGNAME | tr A-Z a-z) data/fruits.txt; echo done'
probe numeric 'chown $(id -u) data/fruits.txt; ls -ln data/fruits.txt'
probe colon-group 'chown :0 data/fruits.txt; ls -l data/fruits.txt'
probe owner-group 'chown $LOGNAME:0 data/fruits.txt; ls -l data/fruits.txt'
probe unknown-user 'chown NOSUCHUSR data/fruits.txt'
probe unknown-group 'chown $LOGNAME:NOSUCHGRP data/fruits.txt'
probe missing 'chown $LOGNAME data/nosuch'
probe recursive 'chown -R $LOGNAME data/sub; ls -l data/sub'
probe usage 'chown'
finish
