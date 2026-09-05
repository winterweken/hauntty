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
      sha256 "5e8d248366cef4467f9dfe2d9dfcb0276e389c8df5b444499f4e32413d241e09"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "0e803955862cc5b3ab26b0d042d07e7a9ac6beddbee3182013c4a26e14a8b66d"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "fcea1490fde626f8f6abaa6cfd82105a86e9f1e78ce02cc399a2d8275c9836e0"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "7a7d817cebefafd8e892b1d8f01e70452aca64d659498a850fa53da3308eb20c"
    end
  end

  def install
    bin.install "hauntty"
  end

  test do
    assert_match "hauntty #{version}", shell_output("#{bin}/hauntty --version")
  end
end
