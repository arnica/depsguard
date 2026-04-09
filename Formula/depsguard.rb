# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.29"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.29/depsguard-macos-arm64.tar.gz"
      sha256 "f7612846d81aa6c4250847584b503392d5fd4585313b806c11fadc11f607e158"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.29/depsguard-macos-amd64.tar.gz"
      sha256 "ed259e88949ec094e2c789ba99fbeb417dd8b6fa0d09d50772d9af893e4f4829"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.29/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "ab415d0f02bd40ffc6c3d510a846ca9eb07b85b9a740554d00ec80c1270a6cd9"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.29/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "8b7f75f04037bda3ff01b73fe7c68129651ae4a337357fa454a0ba77706db8cd"
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
