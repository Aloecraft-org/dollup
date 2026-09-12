ROOT_DIR:=$(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))))
__TECHNO_PROJECT_FILE:=${ROOT_DIR}/.technoproj

# script/version.mk is technoproj's, placed by `technoproj sync` and checked
# by `technoproj check`; it is not edited here. It provides `version`,
# `dev-tag`, `inc_maj`, `inc_min`, `inc_pat`, `set_pre KIND= N=` and
# `clear_pre`. The two below are conveniences over its variables.
-include ${ROOT_DIR}/script/version.mk

echo:
	@echo VERSION: ${__VERSION_FULL}
	@echo TAG: ${__TAG}

# The tag body and the tag, one per line, for scripts.
tag:
	@echo ${__TAG}

semver:
	@echo ${__SEMVER}

# The changelog and the tree agree, and the generated files are fresh:
# what CI runs.
changelog-check:
	./script/changelog.py validate
	./script/changelog.py check
	./script/changelog.py consistency

.PHONY: echo tag semver changelog-check
