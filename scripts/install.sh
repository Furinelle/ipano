#!/bin/sh
# ipano 一键安装:下载对应架构的静态 musl 二进制(无需 Rust / 不在本机编译)。
# 用法:  curl -fsSL https://raw.githubusercontent.com/Furinelle/ipano/main/scripts/install.sh | sh
#         curl -fsSL .../install.sh | sh -s -- ~/bin     # 指定安装目录(默认 /usr/local/bin)
set -e

REPO="Furinelle/ipano"
BINDIR="${1:-/usr/local/bin}"

[ "$(uname -s)" = "Linux" ] || { echo "本脚本仅支持 Linux;macOS 请用源码编译(cargo build --release)" >&2; exit 1; }

case "$(uname -m)" in
  x86_64 | amd64)  TARGET="x86_64-unknown-linux-musl" ;;
  aarch64 | arm64) TARGET="aarch64-unknown-linux-musl" ;;
  *) echo "不支持的架构:$(uname -m)" >&2; exit 1 ;;
esac

URL="https://github.com/$REPO/releases/latest/download/ipano-$TARGET.tar.gz"
echo "下载 $URL"
tmp="$(mktemp -d)"
curl -fsSL "$URL" | tar xz -C "$tmp"
chmod +x "$tmp/ipano"

if mv "$tmp/ipano" "$BINDIR/ipano" 2>/dev/null; then
  :
else
  echo "需要权限写入 $BINDIR,改用 sudo"
  sudo mv "$tmp/ipano" "$BINDIR/ipano"
fi
rm -rf "$tmp"

echo "已安装到 $BINDIR/ipano"
"$BINDIR/ipano" --version 2>/dev/null || true
echo "提示:三网回程路由需特权 —— sudo ipano --route  或  sudo setcap cap_net_raw+ep $BINDIR/ipano"
