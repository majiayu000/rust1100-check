#!/usr/bin/env bash
# Minimized from Poudlardo/Allocine2Letterboxd build.sh
RUST_VERSION=$(rustc --version | cut -d' ' -f2)
if [[ "$RUST_VERSION" < "1.70.0" ]]; then
    echo "Rust 1.70.0 or newer is required" >&2
    exit 1
fi
