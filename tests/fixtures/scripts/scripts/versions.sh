#!/usr/bin/env bash
# Minimized from Monles/rust-malware-re-comparison verification/03_signature_versions.sh
grep -rhoE '1\.[0-9]{2}\.[0-9]+' "$DATA"/build_refs*.sh | sort -uV
