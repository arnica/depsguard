# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.34"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  on_macos do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.34/depsguard-macos-arm64.tar.gz"
      sha256 "94e44559f908565e931e2a3bd00df4d69372fb21f6cddcddb9f4ad7e361e0e53"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.34/depsguard-macos-amd64.tar.gz"
      sha256 "1e5660f14f074f30b97f7e93df60bdc4db401258682eb6e9c04b7f3e0890fca2"
    else
      odie "depsguard: unsupported macOS architecture: #{Hardware::CPU.arch}"
    end
  end

  on_linux do
    if Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.34/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "3abdaf303e212b82ca8b0599b20c99364845046302ba12bc7e99c46292416c3d"
    elsif Hardware::CPU.intel?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.34/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "01bcef0a2809f92a2ef23cd10cd24fead2091300a8190f51a3444cf0d8400694"
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
