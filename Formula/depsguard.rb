# frozen_string_literal: true

class Depsguard < Formula
  desc "Harden package manager configs against supply chain attacks, built by Arnica"
  homepage "https://depsguard.com"
  url "https://github.com/arnica/depsguard/archive/refs/tags/v0.1.5.tar.gz"
  sha256 "ebf3504b82a7595daf5ea9cfa15d19af814b678619c6a2c9ad4bbe5dc8d78c8e"
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
