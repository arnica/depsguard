# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.28"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.28/depsguard-macos-arm64.tar.gz"
      sha256 "a6270d81d50f806292c3078bd33378ff31990a566e62c93821184e8392136b2c"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.28/depsguard-macos-amd64.tar.gz"
      sha256 "d88605cfdad0cbf84ad15403066b6fd3a6f40fd4758e473a9f4aaf2291a06171"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.28/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "83bc38805f8f09acd9eb4eebfb5e3da435b01ca4c41e147a2f9328dde69950d8"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.28/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "2eeaa5cc71c8ba1c349f68b66cc6efc7446a86a1191bcb09dd7913dda0894a4f"
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
