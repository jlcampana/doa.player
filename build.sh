#!/usr/bin/env bash
# Compiles the player. Usage: ./build.sh [--debug] [--run [player arguments]]
set -euo pipefail
cd "$(dirname "$0")"

profile=release
if [[ "${1:-}" == "--debug" ]]; then
    profile=debug
    shift
fi

run=false
if [[ "${1:-}" == "--run" ]]; then
    run=true
    shift
fi

if [[ "$profile" == release ]]; then
    cargo build --release
else
    cargo build
fi

binary="target/$profile/doa_player"
echo "Built $PWD/$binary"

if [[ "$run" == true ]]; then
    exec "./$binary" "$@"
fi
