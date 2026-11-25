#!/bin/sh
# Exports the site of Facile (site/) to static files, for GitHub Pages while no
# server runs it: every page, the assets, the islands (facile.js, facile.wasm and
# the components in Rythme), robots.txt, sitemap.xml and the manifest.
#
#   scripts/export-pages.sh [OUT]        (by default dist/)
#
# BASE (by default /Facile) is put before every path of the site, as Pages
# serves it under https://maxlestage.github.io/Facile; SITE_URL is the address
# of the site, for its canonical links, Open Graph and the sitemap.
# Needs wget and perl.
set -eu
cd "$(dirname "$0")/.."
ROOT=$(pwd)
OUT=${1:-dist}
BASE=${BASE:-/Facile}
SITE_URL=${SITE_URL:-https://maxlestage.github.io$BASE}
PORT=${PORT:-3999}

cargo build --release --bin facile
SITE_URL=$SITE_URL PORT=$PORT target/release/facile serve site > /dev/null 2>&1 &
server=$!
trap 'kill $server 2> /dev/null || true' EXIT
for _ in $(seq 1 60); do
  curl -fs -o /dev/null "http://localhost:$PORT/health" && break
  sleep 1
done

rm -rf "$OUT"
mkdir -p "$OUT"
cd "$OUT"
host="http://localhost:$PORT"
# The pages, by following their links, with what they need.
wget -q --mirror --adjust-extension --page-requisites --no-host-directories -e robots=off \
  "$host/" "$host/en/" || true
# What the pages ask for from their scripts.
extra="/_facile/facile.wasm /_facile/facile.css /_facile/icon.svg /robots.txt /sitemap.xml /manifest.webmanifest"
for f in "$ROOT"/site/client/*.ry; do
  extra="$extra /_facile/client/$(basename "$f")"
done
for f in $(cd "$ROOT/site/public" && find . -type f | sed 's|^\./||'); do
  extra="$extra /static/$f"
done
for path in $extra; do
  mkdir -p ".$(dirname "$path")"
  curl -fs -o ".$path" "$host$path" || rm -f ".$path"
done
# A page that is not there.
curl -s -o 404.html "$host/404" || true
# /docs and /en/docs send to the first page of the documentation.
for lang in "" /en; do
  page="$BASE$lang/docs/start"
  mkdir -p ".$lang/docs"
  printf '<!doctype html><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=%s"><link rel="canonical" href="%s"><a href="%s">%s</a>\n' \
    "$page" "$page" "$page" "$page" > ".$lang/docs/index.html"
done

# Every path of the site starts with BASE.
find . -type f \( -name '*.html' -o -name '*.js' -o -name '*.css' -o -name '*.webmanifest' \) -print0 |
  xargs -0 perl -pi -e '
    s{\b(href|src|data-src|action|poster)="/(?!/)}{$1="'"$BASE"'/}g;
    s{(["'\''`])/(_facile|static|docs|en|benchmark|legal|privacy|manifest\.webmanifest)(?=[/"'\''`?#])}{$1'"$BASE"'/$2}g;
    s{(["'\''`])/(["'\''`])}{$1'"$BASE"'/$2}g;
    s{url\((["'\'']?)/(?!/)}{url($1'"$BASE"'/}g;
  '
touch .nojekyll
echo "$(find . -type f | wc -l) files in $OUT, for $SITE_URL"
