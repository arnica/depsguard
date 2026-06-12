# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.36"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.36/depsguard-macos-arm64.tar.gz"
      sha256 "587d07c19019025a100f8107d05d1db11de9f9dafdbc0cae71da6ad1e8bb9b18"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.36/depsguard-macos-amd64.tar.gz"
      sha256 "2c67cda25a4179785210a9e66cfbf021d21e705c0c59277fa3efee41843abdfe"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.36/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "9551d8eafcbed3cda39872b24264bae2c94c6255cf656b9817d2ccf4a8a3c3ce"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.36/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "9c14ff9a611fba286e65b83373f3bcf5ee556c13a6459564419d5e2a7d15bcde"
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
