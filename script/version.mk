# version.mk -- the version, spelled every way it is spelled, from .technoproj.
#
# .technoproj holds the only version numbers a human edits
# (doc/ALIGNMENT.md §2): major, minor, patch, and `pre`, which is null for a
# release or {"kind": "dev|alpha|beta|rc", "n": N}. Everything else derives:
#
#   __VERSION       PEP 440       0.4.0, 0.4.0rc1, 0.4.0.dev7
#   __VERSION_SEMVER  SemVer      0.4.0, 0.4.0-rc.1, 0.4.0-dev.7
#   __TAG           git tag       v0.4.0, v0.4.0-rc.1, v0.4.0-dev.7
#
# Needs jq. Included by the Makefile with __TECHNO_PROJECT_FILE set.
__VER_MAJ:=$(shell jq -r '.TECHNO_VERSION.major' ${__TECHNO_PROJECT_FILE})
__VER_MIN:=$(shell jq -r '.TECHNO_VERSION.minor' ${__TECHNO_PROJECT_FILE})
__VER_PAT:=$(shell jq -r '.TECHNO_VERSION.patch' ${__TECHNO_PROJECT_FILE})
__VER_PRE_KIND:=$(shell jq -r '.TECHNO_VERSION.pre.kind // empty' ${__TECHNO_PROJECT_FILE})
__VER_PRE_N:=$(shell jq -r '.TECHNO_VERSION.pre.n // empty' ${__TECHNO_PROJECT_FILE})
__VERSION_BASE:=${__VER_MAJ}.${__VER_MIN}.${__VER_PAT}

# PEP 440 spells the kinds a, b, rc and .dev; SemVer spells them in full
# with a dot before the number -- the dot is not decoration (ALIGNMENT §1).
ifeq (${__VER_PRE_KIND},)
__VERSION:=${__VERSION_BASE}
__VERSION_SEMVER:=${__VERSION_BASE}
else ifeq (${__VER_PRE_KIND},dev)
__VERSION:=${__VERSION_BASE}.dev${__VER_PRE_N}
__VERSION_SEMVER:=${__VERSION_BASE}-dev.${__VER_PRE_N}
else ifeq (${__VER_PRE_KIND},alpha)
__VERSION:=${__VERSION_BASE}a${__VER_PRE_N}
__VERSION_SEMVER:=${__VERSION_BASE}-alpha.${__VER_PRE_N}
else ifeq (${__VER_PRE_KIND},beta)
__VERSION:=${__VERSION_BASE}b${__VER_PRE_N}
__VERSION_SEMVER:=${__VERSION_BASE}-beta.${__VER_PRE_N}
else ifeq (${__VER_PRE_KIND},rc)
__VERSION:=${__VERSION_BASE}rc${__VER_PRE_N}
__VERSION_SEMVER:=${__VERSION_BASE}-rc.${__VER_PRE_N}
else
$(error .technoproj: pre.kind '${__VER_PRE_KIND}' is not one of dev, alpha, beta, rc)
endif
__TAG:=v${__VERSION_SEMVER}

version:
	@echo ${__VERSION}

tag:
	@echo ${__TAG}

# The next free dev tag for the base version, allocated from the tags that
# exist rather than stored in the tree: a counter in .technoproj would mean
# a commit on every nightly, and two branches could collide on one number.
# Global and monotonic per repository; never reused.
dev-tag:
	@n=$$(git tag --list 'v*-dev.*' | sed -n 's/.*-dev\.\([0-9][0-9]*\)$$/\1/p' | sort -n | tail -1); \
	echo "v${__VERSION_BASE}-dev.$$(( $${n:-0} + 1 ))"

define __edit_technoproj
	tmp=$$(mktemp) && jq $(1) ${__TECHNO_PROJECT_FILE} > "$$tmp" && mv "$$tmp" ${__TECHNO_PROJECT_FILE}
endef

# Bumping a digit clears the prerelease: a new base is a new release line.
inc_maj:
	$(call __edit_technoproj,'.TECHNO_VERSION.major += 1 | .TECHNO_VERSION.minor = 0 | .TECHNO_VERSION.patch = 0 | .TECHNO_VERSION.pre = null')

inc_min:
	$(call __edit_technoproj,'.TECHNO_VERSION.minor += 1 | .TECHNO_VERSION.patch = 0 | .TECHNO_VERSION.pre = null')

inc_pat:
	$(call __edit_technoproj,'.TECHNO_VERSION.patch += 1 | .TECHNO_VERSION.pre = null')

# make set_pre kind=rc n=1
set_pre:
	@test -n "${kind}" -a -n "${n}" || { echo "usage: make set_pre kind=<dev|alpha|beta|rc> n=<N>" >&2; exit 2; }
	tmp=$$(mktemp) && jq --arg kind "${kind}" --argjson n "${n}" \
	  '.TECHNO_VERSION.pre = {kind: $$kind, n: $$n}' ${__TECHNO_PROJECT_FILE} > "$$tmp" \
	  && mv "$$tmp" ${__TECHNO_PROJECT_FILE}

clear_pre:
	$(call __edit_technoproj,'.TECHNO_VERSION.pre = null')

.PHONY: version tag dev-tag inc_maj inc_min inc_pat set_pre clear_pre
