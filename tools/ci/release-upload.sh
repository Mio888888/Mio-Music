#!/usr/bin/env bash
# Uploads files to a GitHub release by id (a draft has no tag to address it by), replacing any
# asset of the same name. Needs GITHUB_TOKEN and GITHUB_REPOSITORY.
#   release-upload.sh <release-id> <file>...
set -euo pipefail
release_id=$1
shift
api=https://api.github.com/repos/$GITHUB_REPOSITORY
for file in "$@"; do
  name=$(basename "$file")
  echo "Uploading $name to release $release_id"
  for attempt in $(seq 1 10); do
    existing=$(
      curl --fail-with-body --silent --show-error \
        -H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: application/vnd.github+json" \
        "$api/releases/$release_id/assets?per_page=100" |
        jq -r --arg name "$name" 'first(.[] | select(.name == $name) | .id) // empty'
    )
    if [ -n "$existing" ]; then
      curl --fail-with-body --silent --show-error -X DELETE \
        -H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: application/vnd.github+json" \
        "$api/releases/assets/$existing"
    fi
    if curl --fail-with-body --silent --show-error --retry 3 --retry-delay 5 \
      -X POST -H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: application/vnd.github+json" \
      -H "Content-Type: application/octet-stream" --data-binary "@$file" \
      "https://uploads.github.com/repos/$GITHUB_REPOSITORY/releases/$release_id/assets?name=$name" > /dev/null; then
      break
    fi
    if [ "$attempt" -eq 10 ]; then
      echo "::error::Failed to upload $name"
      exit 1
    fi
    echo "Retrying in 20s... ($attempt/10)"
    sleep 20
  done
done
