#!/usr/bin/env bash
# Groups the commits since the last version tag by conventional commit type.
# Needs full history: check out with fetch-depth: 0.
# usage: release-notes.sh <tag being cut>
set -euo pipefail

TAG="${1:-}"
# The newest other tag in this history; a tag from rewritten history is not one.
PREV_TAG=""
for t in $(git tag -l 'v*' --sort=-v:refname); do
  if [ "$t" != "$TAG" ] && git merge-base --is-ancestor "$t" HEAD; then
    PREV_TAG=$t
    break
  fi
done
RANGE=HEAD
[ -n "$PREV_TAG" ] && RANGE="${PREV_TAG}..HEAD"

KNOWN='feat|fix|perf|refactor|docs|test|style|ci|chore|build|revert'

# GitHub turns `@name` in a release into a mention; a code span stops it.
subjects() {
  git log "$RANGE" --no-merges --pretty=format:'%s' \
    | sed -E 's/(^|[^`[:alnum:]_])@([A-Za-z_][A-Za-z0-9_-]*)/\1`@\2`/g'
}

section() {
  subjects \
    | grep -E "^${1}(\([^)]*\))?: " \
    | sed -E "s/^${1}\(([^)]*)\): /- **\1:** /; s/^${1}: /- /" \
    || true
}

BREAKING=$(subjects \
  | grep -E "^(${KNOWN})(\([^)]*\))?!: " \
  | sed -E "s/^(${KNOWN})\(([^)]*)\)!: /- **\2:** /; s/^(${KNOWN})!: /- /" \
  || true)

FEATS=$(section feat)
FIXES=$(section fix)
PERFS=$(section perf)
REFACTORS=$(section refactor)
DOCS=$(section docs)
TESTS=$(section test)
INTERNAL=$(printf '%s\n' "$(section chore)" "$(section style)" "$(section ci)" \
  "$(section build)" "$(section revert)" | grep -v '^$' || true)
OTHERS=$(subjects | grep -Ev "^(${KNOWN})(\([^)]*\))?!?: " | sed 's/^/- /' || true)

emit() {
  [ -n "$2" ] || return 0
  echo "### $1"
  echo "$2"
  echo ""
}

notes() {
  echo "## What's Changed"
  echo ""
  emit "Breaking Changes" "$BREAKING"
  emit "Features" "$FEATS"
  emit "Bug Fixes" "$FIXES"
  emit "Performance" "$PERFS"
  emit "Refactor" "$REFACTORS"
  emit "Documentation" "$DOCS"
  emit "Tests" "$TESTS"
  emit "Internal" "$INTERNAL"
  emit "Other" "$OTHERS"
  if [ -n "$PREV_TAG" ] && [[ "$TAG" == v* ]] && [ -n "${GITHUB_REPOSITORY:-}" ]; then
    echo "**Full Changelog**: https://github.com/${GITHUB_REPOSITORY}/compare/${PREV_TAG}...${TAG}"
  fi
}

# GitHub caps a release body at 125000 characters. awk keeps whole lines.
notes | awk 'BEGIN { limit = 120000 }
  size + length($0) + 1 > limit { print "\nCut short; see the commit log."; exit }
  { size += length($0) + 1; print }'
