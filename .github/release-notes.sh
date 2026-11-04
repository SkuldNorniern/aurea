#!/bin/bash
# Prints release notes for the commits since the last version tag.
# usage: release-notes.sh <tag being cut>
PREV_TAG=$(git tag -l 'v*' --sort=-v:refname | grep -v "^${1}$" | head -1)
RANGE=HEAD
[ -n "$PREV_TAG" ] && RANGE="${PREV_TAG}..HEAD"

section() {
  git log "$RANGE" --pretty=format:"%s" \
    | grep -E "^${1}(\([^)]*\))?: " \
    | sed -E "s/^${1}(\([^)]*\))?: /- /"
}

print_section() {
  local items
  items=$(section "$2")
  if [ -n "$items" ]; then
    echo "### $1"
    echo "$items"
    echo ""
  fi
}

OTHERS=$(git log "$RANGE" --pretty=format:"%s" \
  | grep -Ev "^(feat|fix|docs|refactor|ci|chore|style|test|perf|build)(\([^)]*\))?: ")

# GitHub caps a release body at 125000 characters.
{
  echo "## What's Changed"
  echo ""
  print_section "Features" feat
  print_section "Bug Fixes" fix
  print_section "Performance" perf
  print_section "Documentation" docs
  print_section "Refactor" refactor
  print_section "CI" ci
  if [ -n "$OTHERS" ]; then
    echo "### Other"
    echo "$OTHERS" | sed 's/^/- /'
    echo ""
  fi
} | head -c 120000
