# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  url "https://github.com/arnica/depsguard/archive/refs/tags/v0.1.26.tar.gz"
  sha256 "ac81e54ce2f9ddd39efbebda4c14d7f30def1cc8eeb52ea696e87bb829fd9d27"
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
