# Copyright (C) 2026 rerobots, Inc.

Import('env')

import os


if 'REROBOTS_PLATFORMIO' in os.environ:
    env.Replace(UPLOAD_PROTOCOL='custom')
    env.Replace(UPLOADCMD='platformio-proxy c ' + os.environ['REROBOTS_PLATFORMIO'] + ' $PROJECT_CONFIG $SOURCE')
    env.Replace(UPLOAD_PORT='/dev/null')

    def rrserial_cb(*args, **kwargs):
        env.Execute('pio device monitor -p socket://' + os.environ['REROBOTS_SERIAL'])

    env.AddCustomTarget("rrserial", None, rrserial_cb)
