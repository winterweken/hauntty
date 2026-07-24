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
      sha256 "7ec60692b4b24cbf913b600c9f8412eb2b05effab3fdb4b528db0065ce6ca9a1"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "94693c725cf0c282d75af1fa5a41bb89b9c404f6b481ecbd8bd4375ed41a87ea"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0a9018adc83b3c8993863b4b9832851bd644e8522f62f223294c9a86ddc06734"
    end
    on_intel do
      url "https://github.com/winterweken/hauntty/releases/download/v#{version}/hauntty-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "82d8b3e77f0533c189a538009ad791718d791bd2b26d9ce37e1cc158b2e70a2b"
    end
  end

  def install
    bin.install "hauntty"
  end

  test do
    assert_match "hauntty #{version}", shell_output("#{bin}/hauntty --version")
  end
end
