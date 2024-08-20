#!/usr/bin/env python
"""
Copyright (C) 2021 rerobots, Inc.
"""
from datetime import datetime
import os
import sys
import time

import requests


if __name__ == '__main__':
    double_check_interval = 60*5  # 5 minutes
    misty_ip_addr = os.environ.get('REROBOTS_MISTY_IP') or sys.argv[1]
    battery_thr = 0.5
    battery_msg_title = 'ALERT: Misty battery level is low!'

    err_message = None
    alt_title = None
    try:
        res = requests.get('http://' + misty_ip_addr + '/api/battery', timeout=5)
        if not res.ok:
            err_message = 'Received not OK response: {}'.format(res)
        else:
            payload = res.json()
            if payload['result']['chargePercent'] < battery_thr:
                alt_title = battery_msg_title
                err_message = 'Misty battery level is {} (below threshold of {})'.format(payload['result']['chargePercent'], battery_thr)
    except Exception as err:
        err_message = 'Exception raised while trying to contact Misty: {}: {}'.format(type(err), err)

    if err_message is not None:
        print('failed to communicate with Misty at: {}'.format(datetime.now()))
        try:
            if alt_title:
                title = alt_title
            else:
                title = 'ALERT: Misty is not responding!'
            print(title)
            print(err_message)
            sys.exit(1)
        except Exception as err:
            print('error ({}: {}) while sending alert; sleeping and trying later'.format(type(err), err))
