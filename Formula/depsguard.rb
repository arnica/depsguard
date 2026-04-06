# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  url "https://github.com/arnica/depsguard/archive/refs/tags/v0.1.13.tar.gz"
  sha256 "dfe3c5f529f6a202fde53da4b96ad91407cb00529967bee39272d5c9da8485cd"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "depsguard", shell_output("#{bin}/depsguard --help")
  end
end
