class Awen < Formula
  desc "Terminal Intelligence Layer - Smart when you need it, silent when you don't"
  homepage "https://github.com/zzf2333/Awen"
  version "VERSION"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/zzf2333/Awen/releases/download/vVERSION/awen-VERSION-aarch64-apple-darwin.tar.gz"
      sha256 "SHA256_ARM64_DARWIN"
    end
    on_intel do
      url "https://github.com/zzf2333/Awen/releases/download/vVERSION/awen-VERSION-x86_64-apple-darwin.tar.gz"
      sha256 "SHA256_X86_64_DARWIN"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/zzf2333/Awen/releases/download/vVERSION/awen-VERSION-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "SHA256_ARM64_LINUX"
    end
    on_intel do
      url "https://github.com/zzf2333/Awen/releases/download/vVERSION/awen-VERSION-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "SHA256_X86_64_LINUX"
    end
  end

  def install
    bin.install "awen"
    (share/"awen").install "awen.zsh"
  end

  def caveats
    <<~EOS
      To activate Awen, add this to your ~/.zshrc:

        source #{opt_share}/awen/awen.zsh

      Then restart your shell or run: source ~/.zshrc

      Optional dependencies for best performance:
        brew install jq     # More robust JSON handling
        brew install socat  # Faster socket communication
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/awen --version")
  end
end
