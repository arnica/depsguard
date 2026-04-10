# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.30"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.30/depsguard-macos-arm64.tar.gz"
      sha256 "74363374559c5eaa4431a436862bf40424df53ee99cb6c2878cc559e1ad12940"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.30/depsguard-macos-amd64.tar.gz"
      sha256 "d4b1aaaeb2488ef45d2eba30d99729684b5f7d473fafc02e06d477d2f2340dce"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.30/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "0ffbaf6c5ed0c24c4d07be6a8b1fba3e84901c38091e2f4a8b71b0007e96d063"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.30/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "c50ccb4a9b28d97114bd9b7cbf8bcd44b29f279a5556725ed13b070f7c1a6508"
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
