#!/bin/sh
# husk-cli installer — prebuilt cloud-only binary from GitHub Releases.
# Usage: curl -fsSL https://raw.githubusercontent.com/mcpe500/husk-cli/main/install.sh | sh
# Full build (embedded MiniCPM worker):
#   LLAMA_CMAKE_ARGS="-DGGML_VULKAN=ON" \
#     cargo install --git https://github.com/mcpe500/husk-cli.git --features local,vulkan
set -eu

repo="mcpe500/husk-cli"

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
        asset="husk-cli-x86_64-unknown-linux-gnu.tar.gz"
        bin="husk-cli"
        ;;
    Darwin-arm64|Darwin-x86_64|Linux-aarch64)
        echo "no prebuilt binary for $(uname -s)-$(uname -m)."
        echo "build from source instead:"
        echo "  cargo install --git https://github.com/$repo.git"
        exit 1
        ;;
    MINGW*|MSYS*|CYGWIN*)
        echo "on Windows, download from https://github.com/$repo/releases/latest"
        echo "  asset: husk-cli-x86_64-pc-windows-msvc.zip"
        exit 1
        ;;
    *)
        echo "unsupported platform: $(uname -s)-$(uname -m)"
        exit 1
        ;;
esac

dest="${HUSK_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$dest"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading $asset ..."
curl -fL "https://github.com/$repo/releases/latest/download/$asset" -o "$tmp/pkg"
tar -xzf "$tmp/pkg" -C "$tmp"
install -m 0755 "$tmp/$bin" "$dest/$bin"

echo "installed: $dest/$bin"
echo "next:"
echo "  export PATH=\"$dest:\$PATH\"   # if not already on PATH"
echo "  husk config preset deepseek"
echo "  husk config set api_key \"\$NETRA_API_KEY\""
echo "  husk chat"
