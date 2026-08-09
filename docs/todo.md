# wml2viewer TODO

- project wml2viewerに関するもののみ

ステータス

- [x] 確認済み / 安定実装
- [+] 実装済み / 今後の拡張余地あり
- [*] 実装済みだが要再確認 or 既知の不具合あり
- [-] 設計保留
- [ ] 未実装

最終整理日: 2026-06-05

# 0.0.18

### iOS 17+

- [+] `codex/ios-mvp`: Files主導のcold/warm import、世代付き読取専用snapshot、Rust起動ブリッジ、タッチ操作、補助ファイラー、SMB2/3 provider境界
- [*] Xcode 17+ SDK/Simulatorでのビルド確認済み。CoreSimulatorService復旧後にiPad acceptanceを再実行
- [+] providerをremote source workerへ接続し、iOS UIからSMB共有を一覧・選択
- [ ] HTTP/WebDAV providerとKeychain credential referenceのRustブリッジ
- [ ] Google Drive等のOAuthクラウドprovider

- [x] エフェクトの修正　次に進むと前に戻るで向きを逆にする（「右から左」なら前に戻るは「左から右」）
- [*] MAC対応
- [*] ARM Windows対応
- [*] Linux(Ubuntu24.04/26.04)のテスト実装(VirtualBox + ComputerUse)
- [*] ARM Linux対応
