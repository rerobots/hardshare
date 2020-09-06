all:
	sphinx-build . _build/
	sphinx-build -b html -D language=es . _build/es

gettext-update:
	sphinx-build -b gettext . _build/gettext
	sphinx-intl update -p _build/gettext -l es

clean:
	@rm -rf _build
