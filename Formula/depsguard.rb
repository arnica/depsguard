# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  url "https://github.com/arnica/depsguard/archive/refs/tags/v0.1.20.tar.gz"
  sha256 "098810e572b176ae9fd00381271ed45bed9a89b8fddeb94a5f3cd9c8c4d79f7d"
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
