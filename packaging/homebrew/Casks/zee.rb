cask "zee" do
  version "0.3.10"
  sha256 "94b605fc202f07b74d1c4092b304f6b195f846cac66cb54a064161274066716f"

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
