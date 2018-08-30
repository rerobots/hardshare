# This Makefile is not used (yet) for making, but rather, to simplify
# invocation of common tasks, especially about testing.
#
#
# SCL <scott@rerobots.net>
# Copyright (C) 2018 rerobots, Inc.

.PHONY: check
check:
	pylint -j 4 -E `find py-ws/hardshare -name \*.py`
	pylint -j 4 -E `find tests -name \*.py`
