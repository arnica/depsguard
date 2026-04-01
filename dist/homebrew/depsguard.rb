# Homebrew formula for depsguard
# To use: brew install arnica/tap/depsguard
# This file is auto-updated by the release workflow.

class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks"
  homepage "https://github.com/arnica/depsguard"
  license "MIT"
  version "0.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/arnica/depsguard/releases/download/v#{version}/depsguard-aarch64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_AARCH64_DARWIN"
    else
      url "https://github.com/arnica/depsguard/releases/download/v#{version}/depsguard-x86_64-apple-darwin.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64_DARWIN"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/arnica/depsguard/releases/download/v#{version}/depsguard-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_AARCH64_LINUX"
    else
      url "https://github.com/arnica/depsguard/releases/download/v#{version}/depsguard-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "PLACEHOLDER_SHA256_X86_64_LINUX"
    end
  end

  def install
    bin.install "depsguard"
  end

  test do
    assert_match "depsguard", shell_output("#{bin}/depsguard --version")
  end
end
