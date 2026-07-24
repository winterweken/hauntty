# Homebrew formula for hauntty.
#
# Place this in a tap repo (e.g. github.com/winterweken/homebrew-tap as
# Formula/hauntty.rb). The release workflow produces the tarballs and their
# .sha256 files; fill in VERSION and the four sha256 values per release, or
# automate with `brew bump-formula-pr`.
class Hauntty < Formula
  desc "TUI theme & settings manager for the Ghostty terminal"
  homepage "https://github.com/winterweken/hauntty"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_AARCH64_DARWIN_SHA256"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_X86_64_DARWIN_SHA256"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_AARCH64_LINUX_SHA256"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_X86_64_LINUX_SHA256"
    end
  end

  def install
    bin.install "hauntty"
  end

  test do
    assert_match "hauntty #{version}", shell_output("#{bin}/hauntty --version")
  end
end
