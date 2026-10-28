#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
command -v cargo >/dev/null || { echo "请先安装官方 Rust 工具链。" >&2; exit 1; }
command -v aidoku >/dev/null || { echo "请先安装 Aidoku 官方 CLI，参见 README.md。" >&2; exit 1; }
rustup target add wasm32-unknown-unknown
for source in antbyw jcomic; do
  (cd "$source" && aidoku package)
done
aidoku verify antbyw/package.aix jcomic/package.aix
aidoku build antbyw/package.aix jcomic/package.aix --name "个人漫画图源" --output public
