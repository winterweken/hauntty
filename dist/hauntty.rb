# Homebrew formula for hauntty.
#
# Place this in a tap repo (e.g. github.com/winterweken/homebrew-tap as
# Formula/hauntty.rb). The release workflow produces the tarballs and their
# .sha256 files; fill in VERSION and the four sha256 values per release, or
# automate with `brew bump-formula-pr`.
class Hauntty < Formula
  desc "TUI theme & settings manager for the Ghostty terminal"
  homepage "https://github.com/winterweken/hauntty"
  version "1.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "ff5e6327e155894fad291d316c6f68360790bc0e68c1aba368e173ed6431984c"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "0b0502aee5ea5135b45e50ce5b5410e92716c3c10a93a3bfdcc793db109aa8b5"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "6b80fe686126e6a9cb4c0d35011e8b47c0aee607840f2ba56457cc0ee5047c31"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "d21f1b3f31c0851ab411f065162112ceb0325514cf3e0136b2260bb9a37435c9"
    end
  end

  def install
    bin.install "hauntty"
  end

  test do
    assert_match "hauntty #{version}", shell_output("#{bin}/hauntty --version")
  end
end
