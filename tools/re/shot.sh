#!/bin/bash
# Renders the viewer's window to PNGs and joins them in a montage. Needs X11 (XWayland), ImageMagick.
# shot.sh OUT "Name|costume|motion|yaw|dist|target" ...   (add EXTRA="--doa2u" for DOA2U)
# Output: $SHOT_DIR/OUT.png, default target/shots.
# BIN picks the binary, default target/release/doa_player. Use target/fast/doa_player for quick builds.
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
S=${SHOT_DIR:-$ROOT/target/shots}; mkdir -p "$S"
out=$1; shift; i=0; files=()
for spec in "$@"; do IFS='|' read n c m y d t <<< "$spec"; f=$S/${out}_$i.png
(cd "$ROOT"; WAYLAND_DISPLAY= timeout 10 ${BIN:-target/release/doa_player} --load "$n" $c $m $y $d $t $EXTRA >$S/${out}_$i.log 2>&1 &)
python3 -c "import time;time.sleep(7.5)"; import -window "DOA Player" $f 2>/dev/null; python3 -c "import time;time.sleep(3)"
magick $f -gravity east -crop 72%x88%+0+0 +repage $f; files+=($f); i=$((i+1)); done
magick montage "${files[@]}" -tile 3x -geometry 620x+2+2 $S/$out.png
