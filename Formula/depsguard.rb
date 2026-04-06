# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  url "https://github.com/arnica/depsguard/archive/refs/tags/v0.1.6.tar.gz"
  sha256 "e8ed111fcf5063b0c3bae5c09728d7343ac21bf26ef2e128cfa56abc3c61eb2d"
  license "MIT"
  head "https://github.com/arnica/depsguard.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", "--path", ".", *std_cargo_args
  end

  test do
    assert_match "depsguard", shell_output("#{bin}/depsguard --help")
  end
end
