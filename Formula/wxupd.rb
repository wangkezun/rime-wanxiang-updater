class Wxupd < Formula
  desc "Cross-platform CLI to keep rime-wanxiang scheme, gram model, and dicts up to date"
  homepage "https://github.com/wangkezun/rime-wanxiang-updater"
  version "0.1.4"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-aarch64-apple-darwin"
      sha256 "bfb06f5391ad6a3b8413756f793171f537d8ea777fe38b733cfa5b2122a40dd2"
    end
    on_intel do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-x86_64-apple-darwin"
      sha256 "a557409bac67b755050ef5c06b2d4186120c87e87b7d6f4f2bca4384e7a376f3"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/wangkezun/rime-wanxiang-updater/releases/download/v#{version}/wxupd-v#{version}-x86_64-unknown-linux-musl"
      sha256 "821ad8bfd9c2c87606138f104e61bcf001c443139de558c364429b09f2257f45"
    end
  end

  def install
    bin.install Dir["wxupd*"].first => "wxupd"
  end

  test do
    assert_match "wxupd #{version}", shell_output("#{bin}/wxupd --version")
  end
end
