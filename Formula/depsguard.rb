# frozen_string_literal: true
# Rendered by .github/workflows/release.yml for automated formula sync PRs.
# Target repository: this repository (`Formula/depsguard.rb`).
class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  version "0.1.27"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.27/depsguard-macos-arm64.tar.gz"
      sha256 "abf46e76f02244e8e3be39c1fcd00e39667affcba97da9c14d82b16dbf55ecd1"
    else
      url "https://github.com/arnica/depsguard/releases/download/v0.1.27/depsguard-macos-amd64.tar.gz"
      sha256 "9f1f19ae89e5c77d75c6feb9e7c07b705e20300ac6d1e1a85b9b3f72c695a8ee"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/arnica/depsguard/releases/download/v0.1.27/depsguard-linux-arm64-gnu.tar.gz"
      sha256 "96ed40402715aec3d2f53ef8c7c0e57adb3831364e059f15ee66f6f79cb7a3d6"
    else
      url "https://github.com/arnica/depsguard/releases/download/v0.1.27/depsguard-linux-amd64-gnu.tar.gz"
      sha256 "b25bced16038a5da871be74f8b233a619dd0142500df1b81b893838ea28bf809"
    end
  end

  def install
    bin.install "depsguard"
  end

  test do
    assert_match "depsguard", shell_output("#{bin}/depsguard --help")
  end
end
