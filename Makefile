# This Makefile is not used (yet) for making, but rather, to simplify
# invocation of common tasks, especially about testing.
#
#
# SCL <scott@rerobots.net>
# Copyright (C) 2018 rerobots, Inc.


.PHONY: check
check:
	cd py-ws/tests && pytest -v -x

# TODO: not --exit-zero ?
.PHONY: checkstyle
checkstyle:
	cd py-ws && pylint --exit-zero --disable=fixme hardshare
	pylint -j 4 -E `find py-ws/tests -name \*.py`
