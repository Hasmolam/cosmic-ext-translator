#!/usr/bin/env bash
# Keeps the cargo target directory below a size limit.
#
#   target-limit.sh [target-dir]
#
# Cargo never deletes old artifacts. Every dependency, feature or crate layout change
# leaves the previous build in target/, and the directory grows without bound. When
# target/ is larger than TARGET_LIMIT_GB (default 30), the incremental caches are
# removed first; if it is still over the limit, `cargo clean` removes everything.
# The next build is then a full rebuild.
set -euo pipefail

dir=${1:-target}
limit_gb=${TARGET_LIMIT_GB:-30}

if ! [[ $limit_gb =~ ^[0-9]+$ ]] || [ "$limit_gb" -eq 0 ]; then
    echo "TARGET_LIMIT_GB must be a positive integer, got '$limit_gb'" >&2
    exit 1
fi
[ -d "$dir" ] || exit 0

size_gb() {
    du -s --block-size=1G "$dir" | cut -f1
}

size=$(size_gb)
[ "$size" -le "$limit_gb" ] && exit 0

echo "$dir is ${size} GB, limit ${limit_gb} GB: removing incremental caches"
find "$dir" -maxdepth 3 -type d -name incremental -prune -exec rm -rf {} +

size=$(size_gb)
[ "$size" -le "$limit_gb" ] && exit 0

echo "$dir is still ${size} GB: running cargo clean"
cargo clean --target-dir "$dir"
