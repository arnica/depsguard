# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.33"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.33/depsguard-macos-arm64.tar.gz"
      sha256 "cbe757dbde48a65abb401e9b24f976458ec4f07b4e07ad939d8cdf9491bdc345"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.33/depsguard-macos-amd64.tar.gz"
      sha256 "de9033fb919a718a5d531aee972c4d8d8bf91de62a4a60e4c45e9c9e0c5ead35"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.33/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "34dffb79c8ae9b35c201b6c0e42a445e4d36e128ac7ffdc05ef0269556ed3cb4"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.33/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "8fcf6720a7672246ad4a4aa775f92690d9e9afdf2629070edefe70366d950f9d"
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
