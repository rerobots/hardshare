# Base image using Ubuntu 18.04
#
#     docker build -t rerobots/hs-generic:latest -f Dockerfile .
#
#
# SCL <scott@rerobots.net>
# Copyright (C) 2018 rerobots, Inc.

FROM ubuntu:18.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get -y install \
       openssh-server \
       openvpn \
       avahi-daemon \
       nano \
    && mkdir -p /var/run/sshd

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get -y install \
       python3 \
       python3-aiohttp

COPY init.sh /sbin/rerobots-hs-init.sh

CMD ["/sbin/rerobots-hs-init.sh"]
