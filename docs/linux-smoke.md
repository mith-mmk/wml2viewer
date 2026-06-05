# Linux smoke test

最終整理日: 2026-06-05

Ubuntu 24.04 / 26.04 の手動確認手順。作業用ファイルは必ず `.test*` 配下に置き、終了後に削除する。

## 前提

- VirtualBox の VM を Ubuntu 24.04 で用意する。
- Ubuntu 26.04 が利用可能な場合は同じ手順を再実行する。
- `.test*` と `test_data/` は `.gitignore` 対象であることを確認する。
- 権利上問題のある画像・アーカイブは commit しない。

## Build

```bash
rustup update stable
cargo test --workspace
cargo check --workspace --examples
cargo build --release
```

## Smoke

1. `.test-smoke/` を作成し、png / jpg / gif / zip / lzh の確認用ファイルを置く。
2. `target/release/wml2viewer .test-smoke` を起動する。
3. 画像が表示されることを確認する。
4. Next / Prev でページ移動し、slide 系 transition の向きが逆になることを確認する。
5. Settings を開き、保存先選択ダイアログが開くことを確認する。
6. zip と lzh を開き、virtual child の next/prev が動くことを確認する。
7. 終了後、`.test-smoke/` を削除する。

## ComputerUse

GUI の自動確認を行う場合は、先に HTTP server や一時プロセスの生存確認を行う。Chrome を使う場合の作業プロファイルは `.test-chrome-profile` とし、実行後に削除する。
