#!/usr/bin/env bash
# Signs updater packages with the project's updater key, as `tauri build` does: writes `<file>.sig`
# beside each file. TAURI_SIGNING_PRIVATE_KEY (key text) and, if the key is protected,
# TAURI_SIGNING_PRIVATE_KEY_PASSWORD come from the environment, the same secrets as Mio-Music.
# With MIO_SIGN_OPTIONAL=1 (test builds that publish nothing) a missing key only skips signing.
set -euo pipefail
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  if [ -n "${MIO_SIGN_OPTIONAL:-}" ]; then
    echo "::warning::TAURI_SIGNING_PRIVATE_KEY is not set; the packages stay unsigned"
    exit 0
  fi
  echo "::error::TAURI_SIGNING_PRIVATE_KEY secret is required: without signatures installed copies cannot update"
  exit 1
fi
# `signer sign` fails on an empty password variable; leave it unset instead.
[ -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ] || unset TAURI_SIGNING_PRIVATE_KEY_PASSWORD
for file in "$@"; do
  npx --yes @tauri-apps/cli@2.12.3 signer sign "$file"
  test -s "$file.sig"
done
