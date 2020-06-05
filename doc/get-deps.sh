#!/bin/sh -e
# Script to fetch and prepare dependencies to build documentation
#
#
# SCL <scott@rerobots.net>
# Copyright (C) 2020 rerobots, Inc.


GLFSVERSION=2.11.0
FNAME=git-lfs-linux-amd64-v${GLFSVERSION}.tar.gz


if [ ! -f $FNAME ]; then
    curl -L -O https://github.com/git-lfs/git-lfs/releases/download/v${GLFSVERSION}/${FNAME}
fi

HASHNOW=`sha512sum $FNAME | cut -d ' ' -f1`
if [ "$HASHNOW" != "ed84947ddf492ef7a9480cb826e2667240743cc10a07e186a5759b0b2961decc176cfb7ede2e21b7e764b200dd0b431297be30597553282ef4da5ce10c2fc4d7" ]; then
    echo "Error: wrong checksum: $HASHNOW"
    exit 1
fi

tar -xzf $FNAME
./git-lfs install
./git-lfs fetch
./git-lfs checkout
