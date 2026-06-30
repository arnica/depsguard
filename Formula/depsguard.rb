# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.39"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.39/depsguard-macos-arm64.tar.gz"
      sha256 "c2032776699a9cd03bbb04d89162040f336b8afbae092494a4da2f328b9e7f7b"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.39/depsguard-macos-amd64.tar.gz"
      sha256 "310d8879d4af7c4014603f7c2ebef752d5379d27f29f4e7b608c693ae09d0448"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.39/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "2428ca0181b852cd37d9de3131427e62ed82de98fca7b4c65c6e23d5e7db034b"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.39/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "2a7fd73bde8b39be9d13f65451a24c486e42c722d2cf54deb160d6cdebf0628e"
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
