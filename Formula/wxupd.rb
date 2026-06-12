class Wxupd < Formula
  desc "Cross-platform CLI to keep rime-wanxiang scheme, gram model, and dicts up to date"
  homepage "https://github.com/wangkezun/rime-wanxiang-updater"
  version "0.1.3"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-aarch64-apple-darwin"
      sha256 "52938f759ad3c44274f2fe6f86594d5b4e24ac6c03db65541880407b2e7d5f5d"
    end
    on_intel do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-x86_64-apple-darwin"
      sha256 "37864055ac0774dd3177e84c28e8b49a5ae22a9f66c94ce41a913ed7f8bd62b5"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-x86_64-unknown-linux-musl"
      sha256 "59bdc0af70681a840ebac4ee856fd2e7a8bf9bde1f360a639e2d73e050a5dae9"
    end
  end

  def install
    bin.install Dir["wxupd*"].first => "wxupd"
  end

  test do
    assert_match "wxupd #{version}", shell_output("#{bin}/wxupd --version")
  end
end
