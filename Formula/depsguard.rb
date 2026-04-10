# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.31"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.31/depsguard-macos-arm64.tar.gz"
      sha256 "4c76f7220b19747874f62423ffe9bb45f52395fbec09b8184974eb019bc118fb"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.31/depsguard-macos-amd64.tar.gz"
      sha256 "94c0236399439747dc5b887b4052ec57d5b8bd2700f1d3af5ae13815d628e098"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.31/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "d63aca7260cf0348c6168b65b6f370b08a4ab8c6305acb2e900064e93df5445e"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.31/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "cdc113c086c62974151a2cf82bc3f2c33c5887fcabfe9e12122a514b4d22ce56"
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
