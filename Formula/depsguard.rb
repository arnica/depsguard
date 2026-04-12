# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.32"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.32/depsguard-macos-arm64.tar.gz"
      sha256 "7fe8b7e9bf593953ec131eedbe48a325608a6b919e18ce7b5955cbaa696a7088"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.32/depsguard-macos-amd64.tar.gz"
      sha256 "b8209dbc7c57f32e0c1a4e85571efee52ee0d88c286e45ba9ebf9e79043e7af0"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.32/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "034091d49e01d35f324f32260bda8800b9dd38923183021f3ab00ebd4cae5532"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.32/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "6c34632ba72da58e78620fec6db7a0d7f6f248e24375202f00bb478057e91de4"
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
