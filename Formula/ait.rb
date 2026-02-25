class Ait < Formula
  desc "CLI tool for tracking AI provider usage"
  homepage "https://github.com/j0nl1/aitracker"
  url "https://github.com/j0nl1/aitracker/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "ait #{version}", shell_output("#{bin}/ait --version")
  end
end
