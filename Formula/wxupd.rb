class Wxupd < Formula
  desc "Cross-platform CLI to keep rime-wanxiang scheme, gram model, and dicts up to date"
  homepage "https://github.com/wangkezun/rime-wanxiang-updater"
  version "0.1.2"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-aarch64-apple-darwin"
      sha256 "a16968949c5ef4f39a5e0468abff6518f2cdf76fa1acdd8ad310be5018f953f4"
    end
    on_intel do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-x86_64-apple-darwin"
      sha256 "4764b70aa8e77c2b2159d373258ed94e77cc1ca389ad38f0f1c5fbd4f0e7d6a7"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-x86_64-unknown-linux-musl"
      sha256 "03b016e332795bdfd38c3a8358617806bdbdf512c79e4fa80e136c41bd898d79"
    end
  end

  def install
    bin.install Dir["wxupd*"].first => "wxupd"
  end

  test do
    assert_match "wxupd #{version}", shell_output("#{bin}/wxupd --version")
  end
end
