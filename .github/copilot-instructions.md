# Copilot Instructions

## プロジェクト概要

**cat-watcher** — Windows 向けファイル常駐監視アプリケーション（Rust 製）。  
特定フォルダをリアルタイム監視し、命名規則（glob / 正規表現）に合致するファイル・フォルダを検知して、コピー・移動・コマンド実行・外部プロセス起動を自動実行する。

補助ツール **csv2toml** で Excel（CSV）管理のルール定義を TOML に変換する。

### ドキュメント

仕様・設計の詳細は以下を参照。このファイルには要点のみ記載する。

| 文書 | パス | 内容 |
|------|------|------|
| 要件定義書 | [`doc/requirements.md`](../doc/requirements.md) | 機能要件・非機能要件 |
| 運用仕様書 | [`doc/specification.md`](../doc/specification.md) | 設定の書き方・運用手順・トラブルシューティング |
| 詳細設計書 | [`doc/detailed-design.md`](../doc/detailed-design.md) | アーキテクチャ・モジュール設計・全仕様 |
| 実装計画書 | [`doc/implementation-plan.md`](../doc/implementation-plan.md) | フェーズ分割・テスト戦略・設計補足 Q&A |
| ブランチ戦略 | [`doc/branch-strategy.md`](../doc/branch-strategy.md) | Phase Branch Flow・命名規則・工数見積もり |
| 設計メモ | [`doc/設計下書き/design.md`](../doc/設計下書き/design.md) | 初期設計の検討メモ（参考資料） |

### 現状

- スケルトン段階（`src/main.rs` のみ）。実装開始時に Cargo ワークスペース構成へ移行する
- ドキュメント完備（要件定義・設計・実装計画・ブランチ戦略 すべて v1.0）
- 実装フェーズ 1（Cargo ワークスペース化 + config モジュール）から着手予定

---

## 成果物

| バイナリ | 説明 |
|---------|------|
| `cat-watcher.exe` | ファイル監視アプリケーション本体 |
| `csv2toml.exe` | CSV→TOML 変換ツール |

---

## リポジトリ構成（計画）

```
Cargo.toml              # ワークスペース定義
cat-watcher/            # 監視アプリ本体
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs         # CLI パース、シグナル、起動シーケンス
│       ├── config.rs       # TOML 読み込み・デシリアライズ・バリデーション
│       ├── watcher.rs      # notify による監視セットアップ・制御
│       ├── router.rs       # デバウンス、target/hidden/pattern フィルタ、ルール振り分け
│       ├── actions/
│       │   ├── mod.rs          # Action トレイト、チェーン実行制御
│       │   ├── copy.rs         # コピー（リトライ付き await）
│       │   ├── move_file.rs    # 移動（異ボリュームは copy→delete フォールバック）
│       │   ├── command.rs      # シェル経由コマンド（fire-and-forget）
│       │   └── execute.rs      # CreateProcessW 直接起動（fire-and-forget）
│       ├── placeholder.rs  # プレースホルダ解析・展開・バリデーション
│       └── error.rs        # エラー型定義
csv2toml/               # CSV→TOML 変換ツール
│   ├── Cargo.toml
│   └── src/
│       └── main.rs         # CSV パース・TOML 変換・バリデーション
config/                 # 設定ファイルサンプル
doc/                    # ドキュメント
tool/                   # 補助スクリプト（テスト用ファイル生成等）
```

---

## ビルド・実行コマンド

```powershell
# ビルド（デバッグ）
cargo build

# ビルド（リリース）
cargo build --release

# cat-watcher 実行
cargo run -p cat-watcher -- --global config/global.toml --rules config/rules.toml

# csv2toml 実行
cargo run -p csv2toml -- --input config/rules.csv --output config/rules.toml

# テスト
cargo test

# 個別クレートのテスト
cargo test -p cat-watcher
cargo test -p csv2toml

# チェック（コンパイルのみ、高速）
cargo check

# Lint
cargo clippy -- -D warnings

# フォーマット
cargo fmt
```

---

## 主要クレート

| 用途 | クレート |
|------|---------|
| ファイル監視 | `notify` |
| 非同期ランタイム | `tokio` |
| 設定ファイル | `toml` + `serde` |
| glob マッチ | `globset` |
| 正規表現 | `regex` |
| ロギング | `tracing` + `tracing-subscriber` + `tracing-appender` |
| CLI 引数 | `clap` |
| シグナルハンドリング | `tokio::signal` |
| CSV パース | `csv` + `serde` |
| ハッシュ（完全性検証） | `blake3` |

---

## アーキテクチャ

```
global.toml + rules.toml
    │
    ▼
Config Loader（バリデーション含む）
    │
    ▼
Watcher（notify / ReadDirectoryChangesW）
    │ ファイルシステムイベント
    ▼
Event Router
    │  デバウンス（500ms 固定）
    │  → target フィルタ（file / directory / both）
    │  → include_hidden フィルタ（FILE_ATTRIBUTE_HIDDEN）
    │  → patterns / exclude_patterns / regex マッチ
    │  → events 積集合判定
    ▼
Action Executor（tokio::spawn）
    │  copy | move（await）| command | execute（fire-and-forget）
    │  アクションチェーン（順次実行、エラー時中断）
    ▼
Logger（tracing → JSON 構造化ログ → ファイル出力のみ）
```

> フィルタ適用順: **target → include_hidden → patterns/exclude_patterns/regex → events**

---

## コーディング規約

- **エラー処理**: `anyhow` または独自 `error.rs` で統一。`unwrap()` は原則禁止（テストコードを除く）
- **非同期**: `tokio` async/await。アクション実行は `tokio::spawn` で切り離し、監視スレッドをブロックしない
- **パス操作**: `std::path::PathBuf` / `Path` を使用。文字列リテラルのパス操作は避ける。区切り文字は `/` に統一
- **設定の型**: `serde::Deserialize` を実装した構造体。全フィールド必須（`Option<T>` は使わない）
- **glob / regex**: 起動時に 1 回だけコンパイルし使い回す
- **ログ**: JSON 構造化ログ。コンソール出力なし、ファイル出力のみ

---

## 設計上の重要な決定事項

> 詳細は [`doc/detailed-design.md`](../doc/detailed-design.md) の各セクションを参照。

### 設定ファイル（§3）
- **2 ファイル構成**: `global.toml`（手動編集）+ `rules.toml`（CSV 変換 or 手動編集）
- **デフォルト値なし**: 全項目を明示的に指定必須。省略時はバリデーションエラー
- **ホットリロードなし**: 設定変更時はアプリ再起動
- **相対パスは CWD 基準**

### デバウンス（§7）
- **500ms 固定**（コード内定数、ユーザー設定なし）
- ファイルパス単位でイベント種別の集合を保持し、`events` 設定との積集合で判定
- 積集合が空でなければ 1 回だけアクション実行

### アクション実行（§10）
- `copy` / `move`: **完了を待つ**（await）。「コピー＋完全性検証」を 1 つのリトライ単位とし、I/O エラー・ハッシュ不一致の両方をリトライ
- `command` / `execute`: **fire-and-forget**。タイムアウトなし、終了コード判定なし、stdout/stderr ログ記録なし
- `move` の異ボリューム: `rename` 失敗時は `copy → 元ファイル削除` にフォールバック
- **完全性検証**: `verify_integrity = true` で copy/move 後に BLAKE3 ハッシュ値比較。「コピー＋検証」を 1 リトライ単位とし、不一致時は宛先を削除してリトライ。move の同一ボリューム rename ではスキップ。最終失敗時は不正な宛先を削除（move の異ボリューム時は元ファイル保持）
- アクションチェーンのエラー時は後続を中断

### プレースホルダ（§9）
- PowerShell/.NET 準拠の命名: `{FullName}`, `{DirectoryName}`, `{Name}`, `{BaseName}`, `{Extension}`
- コンテキスト: `{RelativePath}`, `{WatchPath}`, `{Destination}`
- 日時: `{Date}` (YYYYMMDD), `{Time}` (HHmmss), `{DateTime}` (YYYYMMDD_HHmmss)
- エスケープ: `{{` `}}`。未知のプレースホルダは**起動時バリデーションでエラー**
- `{Extension}` はドットなし（例: `csv`）。拡張子なしファイルは空文字列
- `move` 後は後続のパス系プレースホルダが移動先に更新される

### 起動シーケンス（§5）
1. CLI 引数パース → 2. global.toml 読み込み → 3. rules.toml 読み込み → 4. バリデーション（循環参照含む） → 5. ロガー初期化 → 6. glob/regex コンパイル → 7. 監視ディレクトリ存在確認 → 8. **既存ファイル初回スキャン** → 9. Watcher 起動 → 10. イベントループ
- ステートレス: 過去の処理済みファイルの記録は保持しない。再起動のたびに既存ファイルが再処理される

### 設計補足事項（実装計画書 §2 で確定）

> 詳細設計書への反映は別途。実装時はこちらの決定に従う。

| # | 項目 | 決定内容 |
|---|------|----------|
| 1 | `exclude_patterns` | **必須**。空配列 `[]` で明示。`Vec<String>` で受ける |
| 2 | `working_dir` | **必須**（command / execute 両方）。空文字列 `""` 許可（CWD を使用） |
| 3 | 初回スキャン時の events | **events フィルタ適用しない**。patterns/target マッチで無条件実行 |
| 4 | `overwrite = false` スキップ時 | **正常扱い**（チェーン継続）。WARN ログ出力 |
| 5 | patterns/regex マッチ対象 | target 指定の**名前部分**（ファイル名 or ディレクトリ名）にマッチ |
| 6 | `verify_integrity` | **必須**（copy/move 時）。BLAKE3 ハッシュ値比較で完全性検証。同一ボリューム rename ではスキップ |

### 安全性（§13）
- **循環参照**: 起動時バリデーションで 4 パターンを検出（同一ディレクトリ / destination が watch_path 配下 / watch_path が destination 配下 / ルール間相互参照のグラフ循環）
- **シンボリックリンク**: 追跡しない
- **パストラバーサル**: `canonicalize` で正規化して防止
- **コマンドインジェクション**: サニタイズ非実装。設定ファイルの NTFS 権限管理で防御

---

## Windows 固有の注意点

- **監視 API**: `ReadDirectoryChangesW`（ディレクトリ単位ハンドル）。SMB で通知が届かない環境は動作保証外
- **Watcher 統合**: 同一 `watch.path` を監視する複数ルールは OS レベルの Watcher を 1 つにまとめる
- **OS バッファ溢れ**: `notify` の `Rescan` イベント受信時はディレクトリをフルスキャンして差分検出
- **ファイルロック**: `SHARING_VIOLATION` 対策として `retry_count` / `retry_interval_ms` でリトライ
- **隠しファイル**: `include_hidden = false` の場合 `GetFileAttributesW` で `FILE_ATTRIBUTE_HIDDEN` を判定。属性取得失敗時は処理対象とする（安全側）。親フォルダの Hidden 属性は遡ってチェックしない
- **長いパス**: 260 文字超は `\\?\` プレフィクスで対処
- **文字コード**: `OsString` で UTF-16 ⇔ UTF-8 変換。CSV は BOM 付き UTF-8 を想定
- **プロセス起動**: `command` は `shell` 設定に応じて `cmd.exe /C` / `powershell.exe -NoProfile -Command` / `pwsh.exe -NoProfile -Command`。`execute` は `CreateProcessW` 直接起動
- **ファイル名**: 日本語、半角/全角スペースを含むファイル名を動作保証

---

## ブランチ戦略

> 詳細は [`doc/branch-strategy.md`](../doc/branch-strategy.md) を参照。

- **Phase Branch Flow** 採用（GitHub Flow ベース）
- `main` + フェーズ単位の feature branch（`phase/1-config`, `phase/2-watcher` 等）
- 各フェーズ完了時に `main` へマージし、常に動く状態を `main` に保つ

---

## スキル（`.github/skills/`）

| スキル | 用途 | トリガー例 |
|--------|------|------------|
| `doc-coauthoring` | 構造化ドキュメント共同執筆ワークフロー | 「ドキュメントを書きたい」「spec を作成」 |
| `drawio` | draw.io 図の生成・PNG/SVG/PDF エクスポート | 「図を作って」「drawio」「アーキテクチャ図」 |
| `mermaid` | Mermaid 記法の図生成（日本語安全） | 「フローチャート」「シーケンス図」「mermaid」 |
| `skill-creator` | スキルの新規作成・改善・評価 | 「スキルを作りたい」「スキルを改善」 |

---

## ファイル別インストラクション（`.github/instructions/`）

| ファイル | 対象 | 用途 |
|---------|------|------|
| `rust.instructions.md` | `**/*.rs` | Rust コードレビュー・アドバイスガイド。エラーハンドリング・async パターン・serde 規約・Windows API・テストパターン・初心者向けピットフォール集 |

---

## 設定ファイル概要

> 全項目の型・必須/任意は [`doc/detailed-design.md`](../doc/detailed-design.md) §3 を参照。

- **形式**: TOML（`global.toml` + `rules.toml` の 2 ファイル構成）
- **運用フロー**: Excel → CSV → `csv2toml` 変換 → `rules.toml`（`global.toml` は手動編集）
- **全項目必須**（デフォルト値なし）
- **`patterns` と `regex` は排他**: いずれか一方を必ず指定（両方指定・両方省略はエラー）
- **バリデーション**: 必須項目、型別必須フィールド、パス存在確認、循環参照、プレースホルダ、glob/regex 構文を起動時に一括チェック

### target × recursive の挙動

| | `recursive = false` | `recursive = true` |
|---|---|---|
| `target = "file"` | 直下のファイルのみ | 全階層のファイル |
| `target = "directory"` | 直下のフォルダのみ | 全階層のフォルダ |
| `target = "both"` | 直下のファイル＋フォルダ | 全階層のファイル＋フォルダ |

### 終了コード

| コード | 意味 |
|--------|------|
| `0` | 正常終了（グレースフルシャットダウン / `--validate` 成功） |
| `1` | 設定ファイルエラー（パース失敗、バリデーションエラー） |
| `2` | 実行時致命的エラー（監視ディレクトリ消失等） |

---

## 未実装・今後の拡張

- Windows サービス化（`windows-service` クレート）
- コンソール出力（ファイル出力とは別に）
- 条件分岐アクション（ファイルサイズ・拡張子・更新日時）
- デスクトップ / メール通知
- ステータス表示 CLI / HTTP
