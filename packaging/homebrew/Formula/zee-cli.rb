class ZeeCli < Formula
  desc "Lightweight, modern, and lightning-fast terminal text editor built in Rust (CLI / TUI)"
  homepage "https://github.com/kh813/zee"
  version "0.3.10"
  license "Apache-2.0"

  on_macos do
    url "https://github.com/kh813/zee/archive/refs/tags/v0.3.10.tar.gz"
    sha256 "1a43ef2042397d3e327d065543b4cdeb30b2d458bcad1b62db5bdfc77a3757ec"
    depends_on "rust" => :build
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/kh813/zee/releases/download/v0.3.10/zee-cli-v0.3.10-linux-x64.zip"
      sha256 "abe8393a39afbbae0f5b66b716441561e4c2028278ffef04148b14bf41a740f8"
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
