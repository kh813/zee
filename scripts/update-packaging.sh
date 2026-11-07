#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version> (e.g. 0.3.11)"
    exit 1
fi

VERSION="${VERSION#v}"
TAG="v${VERSION}"

echo "Fetching release checksums for ${TAG}..."
CHECKSUMS=$(curl -sSL "https://github.com/kh813/zee/releases/download/${TAG}/SHA256SUMS.txt")

MAC_SHA=$(echo "$CHECKSUMS" | grep "zee-${TAG}-macos-arm64.zip" | awk '{print $1}')
WIN_SHA=$(echo "$CHECKSUMS" | grep "zee-${TAG}-windows-x64.zip" | awk '{print $1}')

if [ -z "$MAC_SHA" ] || [ -z "$WIN_SHA" ]; then
    echo "Error: Could not find sha256 checksums for release ${TAG}."
    exit 1
fi

echo "macOS ARM64 SHA: ${MAC_SHA}"
echo "Windows x64 SHA: ${WIN_SHA}"

# 1. Update Homebrew Tap
TEMP_TAP_DIR=$(mktemp -d)
trap 'rm -rf "${TEMP_TAP_DIR}"' EXIT

echo "Updating Homebrew Tap (kh813/homebrew-tap)..."
git clone "https://github.com/kh813/homebrew-tap.git" "${TEMP_TAP_DIR}"

cat << EOF > "${TEMP_TAP_DIR}/Casks/zee.rb"
cask "zee" do
  version "${VERSION}"
  sha256 "${MAC_SHA}"

  url "https://github.com/kh813/zee/releases/download/v#{version}/zee-v#{version}-macos-arm64.zip"
  name "zee"
  desc "Lightweight, modern, and lightning-fast text editor built in Rust"
  homepage "https://github.com/kh813/zee"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on arch: :arm64

  app "Zee.app"
  binary "#{appdir}/Zee.app/Contents/MacOS/zeeg", target: "zee"

  uninstall quit: "dev.hiroshi.zeeg"

  zap trash: [
    "~/.config/zee",
    "~/Library/Application Support/zee",
    "~/Library/Saved Application State/dev.hiroshi.zeeg.savedState",
  ]
end
EOF

(
    cd "${TEMP_TAP_DIR}"
    git add Casks/zee.rb
    if git diff --staged --quiet; then
        echo "Homebrew Cask is already up to date."
    else
        git commit -m "chore: bump zee cask to ${VERSION}"
        git push origin main
        echo "Homebrew Cask updated to ${VERSION} successfully!"
    fi
)

echo "To submit WinGet update, run:"
echo "  komac update kh813.zee --version ${VERSION} --urls https://github.com/kh813/zee/releases/download/${TAG}/zee-${TAG}-windows-x64.zip -s -t \$(gh auth token)"
