# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.35"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.35/depsguard-macos-arm64.tar.gz"
      sha256 "b794f5526ea5a026ef77724464ecdb7194f5113cd987da46b241cea75e64228c"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.35/depsguard-macos-amd64.tar.gz"
      sha256 "b74ade76ef7d9f19cd23c595a36c4f9e661b5a8d8ae285fcf8930dcf25aefda8"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.35/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "bb45d499cb90dee214dfd289f9b27fdb6c1774cda90bc10e26a8da1022d1adc8"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.35/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "56ed168670002d4787648a70df2ccd233971367989eaa16f287782f326850f06"
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
