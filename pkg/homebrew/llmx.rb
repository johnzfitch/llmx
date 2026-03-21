class Llmx < Formula
  desc "Local-first codebase indexer with semantic search for LLM agents"
  homepage "https://github.com/johnzfitch/llmx"
  url "https://github.com/johnzfitch/llmx/archive/refs/tags/vVERSION_PLACEHOLDER.tar.gz"
  sha256 "SOURCE_SHA256_PLACEHOLDER"
  license "MIT"
  head "https://github.com/johnzfitch/llmx.git", branch: "master"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "ingestor-core"),
           "--locked", "--features", "cli,mcp"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/llmx --version")

    # Test indexing a simple file
    (testpath/"test.rs").write("fn main() {}")
    system "#{bin}/llmx", "index", testpath
  end
end
