ROOT_DIR:=$(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))))
__TECHNO_PROJECT_FILE:=${ROOT_DIR}/.technoproj

-include ${ROOT_DIR}/script/version.mk

echo:
	@echo VERSION: ${__VERSION}
	@echo TAG: ${__TAG}

# The changelog and the tree agree, and the generated files are fresh:
# what CI runs.
changelog-check:
	./script/changelog.py validate
	./script/changelog.py check
	./script/changelog.py consistency

.PHONY: echo changelog-check
