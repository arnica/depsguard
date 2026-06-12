# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.38"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.38/depsguard-macos-arm64.tar.gz"
      sha256 "6e496ce9b4988ec9049039538e69e64ebaa11673b50f9f52f5f442e0377ede85"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.38/depsguard-macos-amd64.tar.gz"
      sha256 "5e6989e94aadb7d579af964e3a0a47545d6666065ef843e9260857ef771e487f"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.38/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "857bc8d5c984f077c705f5490a02fd9bb2454e5d65b43a8824c18d796257dfa5"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.38/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "1645b92a734634df310dce1e0960e07c870124785d9c8fb2848f028ce4753f38"
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
