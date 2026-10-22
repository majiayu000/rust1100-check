#!/usr/bin/env bash
# Correct forms: none of these should be reported.
msrv=$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml)
actual=$(rustc --version | awk '{print $2}')
if [ "$(printf '%s\n%s\n' "$msrv" "$actual" | sort -V | head -n1)" != "$msrv" ]; then exit 1; fi
minor=$(echo "$actual" | cut -d. -f2)
[ "$minor" -ge 100 ] && echo "three-digit minor"
grep -oE '1\.[0-9]+\.[0-9]+' <<< "$actual"
