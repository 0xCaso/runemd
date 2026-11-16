#!/bin/sh
# Renders the 1024 SVG icon master with headless Chrome (it uses SVG filters rsvg gets wrong),
# downscales it into an iconset with sips and packs assets/RuneMD.icns with iconutil.
# 16, 16@2x and 32 px come from a simpler small-size master (bigger MD, no fire), as the decision allows.
set -eu
cd "$(dirname "$0")/.."
icons="$PWD/design/mockups/2026-11-16-brand-icon/icons"
out=assets/RuneMD.icns
chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
render() { # $1 svg, $2 out png
  cat > "$D/icon.html" <<HTML
<!doctype html><html><head><style>html,body{margin:0;background:transparent}img{display:block;width:1024px;height:1024px}</style></head>
<body><img src="file://$1"></body></html>
HTML
  "$chrome" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 \
    --window-size=1024,1024 --default-background-color=00000000 \
    --screenshot="$2" "file://$D/icon.html" >/dev/null 2>&1
  [ "$(sips -g pixelWidth -g hasAlpha "$2" | awk '/pixelWidth|hasAlpha/{print $2}' | tr '\n' ' ')" = "1024 yes " ] \
    || { echo "render failed: $2 is not a 1024 px PNG with alpha" >&2; exit 1; }
}
render "$icons/forge-md-outfit-plate-fire-low.svg" "$D/master.png"
render "$icons/forge-md-outfit-plate-small.svg" "$D/small.png"
set_=$D/RuneMD.iconset; mkdir "$set_"
for s in 16 32 128 256 512; do
  d=$((s * 2))
  m="$D/master.png"; [ $s -le 32 ] && m="$D/small.png"
  sips -z $s $s "$m" --out "$set_/icon_${s}x${s}.png" >/dev/null
  m2="$D/master.png"; [ $d -le 32 ] && m2="$D/small.png"
  if [ $d -eq 1024 ]; then cp "$D/master.png" "$set_/icon_${s}x${s}@2x.png"
  else sips -z $d $d "$m2" --out "$set_/icon_${s}x${s}@2x.png" >/dev/null; fi
done
iconutil -c icns "$set_" -o "$out"
[ -n "${KEEP_ICONSET:-}" ] && cp -R "$set_" "$KEEP_ICONSET"
echo "$out"
