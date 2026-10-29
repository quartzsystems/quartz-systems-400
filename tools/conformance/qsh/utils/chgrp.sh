# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# chgrp: by number, unknown groups, -R, errors.

probe number 'chgrp 0 data/fruits.txt; ls -l data/fruits.txt'
probe unknown 'chgrp NOSUCHGRP data/fruits.txt'
probe recursive 'chgrp -R 0 data/sub; ls -l data/sub'
probe missing 'chgrp 0 data/nosuch'
probe usage 'chgrp 0'
probe no-operands 'chgrp'
finish
