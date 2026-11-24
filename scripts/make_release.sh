#!/bin/sh
# Builds the files for a GitHub Release: build/RuneMD-<version>.zip (ad hoc signed, not notarized) and
# build/RuneMD-<version>-notes.md. Tags and publishes nothing. Audit docs/audit/2026-11-24-publish-readiness.md, gap 2.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
for key in CFBundleShortVersionString CFBundleVersion; do
    plist=$(/usr/libexec/PlistBuddy -c "Print :$key" assets/Info.plist)
    [ "$version" = "$plist" ] || { echo "Cargo.toml $version and Info.plist $key $plist differ" >&2; exit 1; }
done
cargo build --profile perf
SIGN=adhoc scripts/make_app.sh
codesign --verify --strict build/RuneMD.app
zip=build/RuneMD-$version.zip
rm -f "$zip"
ditto -c -k --keepParent build/RuneMD.app "$zip"
sed "s/{{version}}/$version/g" assets/release-notes.md > "build/RuneMD-$version-notes.md"
(cd build && shasum -a 256 "RuneMD-$version.zip") | sed 's/^/sha256 /' >> "build/RuneMD-$version-notes.md"
echo "$zip"
echo "build/RuneMD-$version-notes.md"
