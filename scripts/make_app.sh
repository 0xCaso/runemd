#!/bin/sh
# Impacchetta target/perf/runemd in build/RuneMD.app, firmato "RuneMD Local", e lo registra in Launch Services.
set -eu
cd "$(dirname "$0")/.."
app=build/RuneMD.app
rm -rf "$app"; mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/Fonts"
cp target/perf/runemd "$app/Contents/MacOS/runemd"
cp assets/Info.plist "$app/Contents/Info.plist"
cp assets/fonts/* "$app/Contents/Resources/Fonts/"
KC="$HOME/Library/Keychains/runemd.keychain-db"
[ -f "$KC" ] || { echo "manca $KC: esegui scripts/make_cert.sh" >&2; exit 1; }
security unlock-keychain -p runemd "$KC"
# Firma per hash, dal portachiavi dedicato: il nome da solo può essere ambiguo.
hash=$(security find-identity -p codesigning "$KC" | sed -n 's/^ *[0-9]*) \([0-9A-F]\{40\}\) "RuneMD Local".*/\1/p' | head -1)
[ -n "$hash" ] || { echo "identità RuneMD Local assente in $KC: esegui scripts/make_cert.sh" >&2; exit 1; }
codesign -s "$hash" --keychain "$KC" --force "$app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app"
echo "$app"
