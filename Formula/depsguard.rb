# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.37"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.37/depsguard-macos-arm64.tar.gz"
      sha256 "a543bcabf22334863e52a981697b6c81d930f86e0a9f156f0e5a9161bd81b174"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.37/depsguard-macos-amd64.tar.gz"
      sha256 "881f29a88afcc1aaf8921fd69d88bf586a5e41e8486402f5791dd570f396cbcc"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.37/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "3f672ebcde178c278dbd9a470bbb8743add32ca8b39177adaccc2d8e5792d480"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.37/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "03c79a21962eb239d8e583c710b08226ebc77475573d96a0b7348bc41b55d106"
    else
      odie "depsguard: unsupported Linux architecture: #{Hardware::CPU.arch}"
    end
  end

  depends_on "rust" => :build if build.head?

  def install
    if build.head?
      system "cargo", "install", *std_cargo_args
    else
      bin.install "depsguard"
    end
  end

  test do
    assert_match "depsguard", shell_output("#{bin}/depsguard --help")
  end
end
