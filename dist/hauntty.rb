# Homebrew formula for hauntty.
#
# Place this in a tap repo (e.g. github.com/winterweken/homebrew-tap as
# Formula/hauntty.rb). The release workflow produces the tarballs and their
# .sha256 files; fill in VERSION and the four sha256 values per release, or
# automate with `brew bump-formula-pr`.
class Hauntty < Formula
  desc "TUI theme & settings manager for the Ghostty terminal"
  homepage "https://github.com/winterweken/hauntty"
  version "1.0.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "7c85a1b7f7425430a6d7fa4fc7aaa40f2b6ecba861278b563d0fa722ee06b231"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "5a93bfeacffcef1f572c2082f5046ffe5a1363b491aa572e9262b63d9a839360"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "69b992cc260d0a12dde84434dae1843ddfa7df08ab12e115ba1de263475fa59c"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "26e0e06fd15cd16cf7d3c35ff31c663d2ec7416a1cc0af4b94a9db82ec59b736"
    end
  end

  def install
    bin.install "hauntty"
  end

  test do
    assert_match "hauntty #{version}", shell_output("#{bin}/hauntty --version")
  end
end
