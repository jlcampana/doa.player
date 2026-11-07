#!/usr/bin/env bash
# Compiles the player. Usage: ./build.sh [--debug] [--allow-dead-code] [--run [player arguments]]
set -euo pipefail
cd "$(dirname "$0")"

profile=release
run=false
allow_dead_code=false
while (($#)); do
    case "$1" in
        --debug)
            profile=debug
            shift
            ;;
        --allow-dead-code)
            allow_dead_code=true
            shift
            ;;
        --run)
            run=true
            shift
            break
            ;;
        *)
            echo "Unknown build option: $1" >&2
            echo "Usage: ./build.sh [--debug] [--allow-dead-code] [--run [player arguments]]" >&2
            exit 2
            ;;
    esac
done

if [[ "$allow_dead_code" == true ]]; then
    if [[ "$profile" == release ]]; then
        cargo rustc --release -- -A dead_code
    else
        cargo rustc -- -A dead_code
    fi
elif [[ "$profile" == release ]]; then
    cargo build --release
else
    cargo build
fi

binary="target/$profile/doa_player"
echo "Built $PWD/$binary"

if [[ "$run" == true ]]; then
    exec "./$binary" "$@"
fi
