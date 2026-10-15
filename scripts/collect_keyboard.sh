#!/bin/sh
set -eu
project=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
mkdir -p "$project/analysis/live"
capture=$(mktemp -d "$project/analysis/live/capture-XXXXXX")
cargo build --offline --manifest-path "$project/Cargo.toml" >"$capture/build.log" 2>&1
status=0
"$project/target/debug/aula-f87pro" list >"$capture/devices.txt" 2>&1 || status=1
"$project/target/debug/aula-f87pro" inspect >"$capture/leds.txt" 2>&1 || status=1
for layer in 0 1 2 3; do
    "$project/target/debug/aula-f87pro" keys "$layer" >"$capture/keys-layer$layer.txt" 2>&1 || status=1
done
printf 'Coleta salva em %s\n' "$capture"
exit "$status"
