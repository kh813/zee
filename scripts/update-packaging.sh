#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version> (e.g. 0.3.10)"
    exit 1
fi

VERSION="${VERSION#v}"
TAG="v${VERSION}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Fetching release checksums for ${TAG}..."
CHECKSUMS=$(curl -sSL "https://github.com/kh813/zee/releases/download/${TAG}/SHA256SUMS.txt")

MAC_GUI_SHA=$(echo "$CHECKSUMS" | grep "zee-${TAG}-macos-arm64.zip" | awk '{print $1}' || true)
MAC_CLI_SHA=$(echo "$CHECKSUMS" | grep "zee-cli-${TAG}-macos-arm64.zip" | awk '{print $1}' || true)
LINUX_CLI_SHA=$(echo "$CHECKSUMS" | grep "zee-cli-${TAG}-linux-x64.zip" | awk '{print $1}' || true)
WIN_SHA=$(echo "$CHECKSUMS" | grep "zee-${TAG}-windows-x64.zip" | awk '{print $1}' || true)

# If macOS CLI zip was uploaded via extended-release.yml after SHA256SUMS.txt was generated, compute its SHA256 directly
if [ -z "$MAC_CLI_SHA" ]; then
    MAC_CLI_URL="https://github.com/kh813/zee/releases/download/${TAG}/zee-cli-${TAG}-macos-arm64.zip"
    if curl -fsSLI "$MAC_CLI_URL" >/dev/null 2>&1; then
        echo "Computing SHA256 for ${MAC_CLI_URL}..."
        if command -v sha256sum >/dev/null 2>&1; then
            MAC_CLI_SHA=$(curl -sSL "$MAC_CLI_URL" | sha256sum | awk '{print $1}')
        else
            MAC_CLI_SHA=$(curl -sSL "$MAC_CLI_URL" | shasum -a 256 | awk '{print $1}')
        fi
    fi
fi

if [ -z "$MAC_GUI_SHA" ] || [ -z "$WIN_SHA" ]; then
    echo "Error: Could not find sha256 checksums for release ${TAG}."
    exit 1
fi

echo "macOS ARM64 GUI SHA: ${MAC_GUI_SHA}"
echo "macOS ARM64 CLI SHA: ${MAC_CLI_SHA:-<fallback to source tarball>}"
echo "Linux x64 CLI SHA:   ${LINUX_CLI_SHA}"
echo "Windows x64 GUI SHA: ${WIN_SHA}"

mkdir -p "${REPO_ROOT}/packaging/homebrew/Casks"
mkdir -p "${REPO_ROOT}/packaging/homebrew/Formula"

# 1. Generate Casks/zee.rb (GUI Edition: Zee.app + zeeg command)
cat << EOF > "${REPO_ROOT}/packaging/homebrew/Casks/zee.rb"
cask "zee" do
  version "${VERSION}"
  sha256 "${MAC_GUI_SHA}"

  url "https://github.com/kh813/zee/releases/download/v#{version}/zee-v#{version}-macos-arm64.zip"
  name "zee"
  desc "Lightweight, modern, and lightning-fast text editor built in Rust (Desktop GUI)"
  homepage "https://github.com/kh813/zee"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on arch: :arm64

  app "Zee.app"
  binary "#{appdir}/Zee.app/Contents/MacOS/zeeg", target: "zeeg"

  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-cr", "#{appdir}/Zee.app"]
    system_command "/usr/bin/codesign",
                   args: ["--force", "--deep", "--sign", "-", "#{appdir}/Zee.app"]
  end

  uninstall quit: "dev.hiroshi.zeeg"

  zap trash: [
    "~/.config/zee",
    "~/Library/Application Support/zee",
    "~/Library/Saved Application State/dev.hiroshi.zeeg.savedState",
  ]
end
EOF

# 2. Generate Formula/zee-cli.rb (CLI Edition: zee & zee-cli commands)
if [ -n "$MAC_CLI_SHA" ]; then
    cat << EOF > "${REPO_ROOT}/packaging/homebrew/Formula/zee-cli.rb"
class ZeeCli < Formula
  desc "Lightweight, modern, and lightning-fast terminal text editor built in Rust (CLI / TUI)"
  homepage "https://github.com/kh813/zee"
  version "${VERSION}"
  license "Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/kh813/zee/releases/download/v${VERSION}/zee-cli-v${VERSION}-macos-arm64.zip"
      sha256 "${MAC_CLI_SHA}"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/kh813/zee/releases/download/v${VERSION}/zee-cli-v${VERSION}-linux-x64.zip"
      sha256 "${LINUX_CLI_SHA}"
    end
  end

  def install
    bin.install "zee"
    bin.install_symlink "zee" => "zee-cli"
  end

  test do
    assert_predicate bin/"zee", :exist?
    assert_predicate bin/"zee-cli", :exist?
  end
end
EOF
else
    echo "Computing source tarball SHA256 for ${TAG}..."
    if command -v sha256sum >/dev/null 2>&1; then
        SRC_SHA=$(curl -sSL "https://github.com/kh813/zee/archive/refs/tags/${TAG}.tar.gz" | sha256sum | awk '{print $1}')
    else
        SRC_SHA=$(curl -sSL "https://github.com/kh813/zee/archive/refs/tags/${TAG}.tar.gz" | shasum -a 256 | awk '{print $1}')
    fi
    cat << EOF > "${REPO_ROOT}/packaging/homebrew/Formula/zee-cli.rb"
class ZeeCli < Formula
  desc "Lightweight, modern, and lightning-fast terminal text editor built in Rust (CLI / TUI)"
  homepage "https://github.com/kh813/zee"
  version "${VERSION}"
  license "Apache-2.0"

  on_macos do
    url "https://github.com/kh813/zee/archive/refs/tags/v${VERSION}.tar.gz"
    sha256 "${SRC_SHA}"
    depends_on "rust" => :build
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/kh813/zee/releases/download/v${VERSION}/zee-cli-v${VERSION}-linux-x64.zip"
      sha256 "${LINUX_CLI_SHA}"
    end
  end

  def install
    if File.exist?("Cargo.toml")
      system "cargo", "build", "--release", "-p", "zee-tui", "--bin", "zee"
      bin.install "target/release/zee"
    else
      bin.install "zee"
    end
    bin.install_symlink "zee" => "zee-cli"
  end

  test do
    assert_predicate bin/"zee", :exist?
    assert_predicate bin/"zee-cli", :exist?
  end
end
EOF
fi

# 3. Update Homebrew Tap (kh813/homebrew-tap)
TEMP_TAP_DIR=$(mktemp -d)
trap 'rm -rf "${TEMP_TAP_DIR}"' EXIT

echo "Updating Homebrew Tap (kh813/homebrew-tap)..."
git clone "https://github.com/kh813/homebrew-tap.git" "${TEMP_TAP_DIR}"

mkdir -p "${TEMP_TAP_DIR}/Casks" "${TEMP_TAP_DIR}/Formula"
cp "${REPO_ROOT}/packaging/homebrew/Casks/zee.rb" "${TEMP_TAP_DIR}/Casks/zee.rb"
cp "${REPO_ROOT}/packaging/homebrew/Formula/zee-cli.rb" "${TEMP_TAP_DIR}/Formula/zee-cli.rb"

(
    cd "${TEMP_TAP_DIR}"
    git add Casks/zee.rb Formula/zee-cli.rb
    if git diff --staged --quiet; then
        echo "Homebrew Tap (zee & zee-cli) is already up to date."
    else
        git commit -m "chore: update zee (GUI) and zee-cli (CLI) to ${VERSION}"
        git push origin main
        echo "Homebrew Tap updated to ${VERSION} successfully!"
    fi
)

echo "To submit WinGet update, run:"
echo "  komac update kh813.zee --version ${VERSION} --urls https://github.com/kh813/zee/releases/download/${TAG}/zee-${TAG}-windows-x64.zip -s -t \$(gh auth token)"
