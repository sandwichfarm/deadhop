#!/usr/bin/env bash
# Publishes the npm packages from their built dist/ (run `npm run build` in
# each first, or let the publish workflow do it). The client goes first: the
# framework packages depend on its exact version.
#
# Safe to re-run: a version that is already on npm is skipped. One package
# failing does not stop the others; the script exits non-zero at the end if
# any failed.
#
#   scripts/publish-npm.sh             # needs `npm login` (and NPM_CONFIG_OTP=123456 with 2FA)
#   scripts/publish-npm.sh --dry-run   # what would be published
#
# In GitHub Actions with trusted publishing configured on npmjs.com, no token
# is needed and npm adds provenance itself.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACKAGES=(client react solid svelte)
failed=()
for p in "${PACKAGES[@]}"; do
  dir="$ROOT/packages/$p"
  name="$(node -p "require('$dir/package.json').name")"
  version="$(node -p "require('$dir/package.json').version")"
  if [ "$(npm view "$name@$version" version 2>/dev/null)" = "$version" ]; then
    echo "skip   $name@$version (already on npm)"
    continue
  fi
  if (cd "$dir" && npm publish --access public "$@"); then
    echo "done   $name@$version"
  else
    echo "FAILED $name@$version" >&2
    failed+=("$name")
  fi
done
if [ "${#failed[@]}" -gt 0 ]; then
  echo "not published: ${failed[*]}" >&2
  exit 1
fi
