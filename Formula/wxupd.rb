class Wxupd < Formula
  desc "Cross-platform CLI to keep rime-wanxiang scheme, gram model, and dicts up to date"
  homepage "https://github.com/wangkezun/rime-wanxiang-updater"
  version "0.1.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-aarch64-apple-darwin"
      sha256 "8534371a3a7d1fbb2e7a424cd78334f21beb2e7fc87b9bb674c5db00f160060e"
    end
    on_intel do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-x86_64-apple-darwin"
      sha256 "b70d42f8cc89cc15477e9caaccf524def06063cc2c208bf0b60de57ddca58d8c"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-x86_64-unknown-linux-musl"
      sha256 "399d0824cdf5720618eb8ed47c3b048ca591156f68a833b4fe41b4eef55f8e92"
    end
  end

  def install
    bin.install Dir["wxupd*"].first => "wxupd"
  end

  test do
    assert_match "wxupd #{version}", shell_output("#{bin}/wxupd --version")
  end
end
