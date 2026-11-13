#!/bin/sh
# Impacchetta target/perf/runemd in build/RuneMD.app, firmato "RuneMD Local", e lo registra in Launch Services.
set -eu
cd "$(dirname "$0")/.."
app=build/RuneMD.app
rm -rf "$app"; mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/Fonts"
cp target/perf/runemd "$app/Contents/MacOS/runemd"
cp assets/Info.plist "$app/Contents/Info.plist"
cp assets/fonts/* "$app/Contents/Resources/Fonts/"
security unlock-keychain -p runemd "$HOME/Library/Keychains/runemd.keychain-db" 2>/dev/null || true
codesign -s "RuneMD Local" --force "$app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app"
echo "$app"
