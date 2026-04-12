# 実装計画書 — cat-watcher / csv2toml

| 項目 | 内容 |
|------|------|
| 文書版数 | 1.1 |
| 作成日 | 2026-03-29 |
| 前提 | 詳細設計書 v1.0 / 要件定義書 v1.0 |

---

## 目次

1. [実装者の前提スキル](#1-実装者の前提スキル)
2. [設計補足事項（Q&A 確定分）](#2-設計補足事項qa-確定分)
3. [実装順序の方針](#3-実装順序の方針)
4. [テスト戦略](#4-テスト戦略)
5. [フェーズ一覧（概要）](#5-フェーズ一覧概要)
6. [フェーズ詳細](#6-フェーズ詳細)
7. [フェーズ間の依存関係](#7-フェーズ間の依存関係)

---

## 1. 実装者の前提スキル

| 項目 | レベル |
|------|--------|
| Rust | 初心者（入門書を読んだ程度） |
| async/await（tokio） | 初めて |
| 外部クレート利用 | 初めて |
| Cargo ワークスペース | 初めて |
| Windows API（Rust FFI） | 初めて |

**方針**: 小さく動くものを段階的に拡張する。各フェーズで「ビルドが通り、動作確認できる」状態を作る。

---

## 2. 設計補足事項（Q&A 確定分）

詳細設計書の曖昧な箇所について以下のとおり確定した。設計書への反映は別途行う。

| # | 項目 | 決定内容 |
|---|------|---------|
| 1 | `exclude_patterns` の必須/任意 | **必須**（方式B）。空配列 `exclude_patterns = []` で明示する。「全項目必須」の設計原則に一貫させる。serde では `Vec<String>` で受ける（`Option` は使わない）。csv2toml では CSV 空欄→空配列に変換する |
| 2 | `working_dir` の扱い | **必須フィールド**（command / execute 両方）。空文字列 `""` も許可する。空文字列の場合は watcher プロセスの CWD を使用する。絶対パスを推奨 |
| 3 | 初回スキャン時のイベント種別 | **events フィルタは適用しない**。既存ファイル/フォルダが patterns/target にマッチすれば無条件でアクション実行する（イベント種別の概念がないため） |
| 4 | `overwrite = false` でスキップ時 | **正常扱い**（アクションチェーン継続）。WARN レベルのログを出力する |
| 5 | patterns/regex/exclude_patterns のマッチ対象 | `target` で指定されたもの（ファイル名 or ディレクトリ名）の**名前部分**に対してマッチする |
| 6 | `verify_integrity` の必須/任意 | **必須**（copy/move 時）。「全項目必須」の設計原則に一貫させる。アルゴリズムは BLAKE3（高速・大容量対応）。同一ボリューム move（rename）ではデータ転送がないためスキップ。「コピー＋検証」を 1 つの操作単位としてリトライし（Case C）、I/O エラーとハッシュ不一致は同じ `retry_count` を消費する。不一致時は宛先を削除してリトライ。異ボリューム move の最終失敗時は元ファイルを保持する（データ消失防止） |

---

## 3. 実装順序の方針

### 3.1 全体の順序

```
cat-watcher（Phase 0〜12） → csv2toml（Phase 13〜14）
```

cat-watcher の中では、**起動シーケンス順**に従い上流（設定読み込み）から下流（アクション実行）へ段階的に実装する。

### 3.2 各フェーズの進め方

1. **フェーズ冒頭**: 学ぶべき Rust の概念・クレートの使い方を把握する
2. **コードを書く**: 対象モジュールを実装する
3. **ビルド確認**: `cargo check` / `cargo build` でコンパイルを通す
4. **テスト**: 該当フェーズのユニットテストを書いて `cargo test` で通す
5. **動作確認**: 可能なフェーズでは実際に実行して動作を目視確認する

---

## 4. テスト戦略

### 4.1 テストの種類

| 種類 | 場所 | 対象 | 優先度 |
|------|------|------|--------|
| **ユニットテスト** | 各 `.rs` 内の `#[cfg(test)] mod tests` | 関数単位のロジック検証 | ★★★ 必須 |
| **結合テスト** | `cat-watcher/tests/` ディレクトリ | モジュール連携の検証 | ★★ 推奨 |
| **手動テスト** | `tool/` のスクリプト等で実環境テスト | エンドツーエンド | ★ 最終段階 |

### 4.2 フェーズ別テスト指針

| フェーズ | テスト内容 |
|---------|-----------|
| Phase 2（エラー型） | エラーメッセージの生成・表示 |
| Phase 3（設定読み込み） | 正常 TOML のパース、不正 TOML でのエラー、必須項目欠落 |
| Phase 5（バリデーション） | 各バリデーションルールの正常系・異常系。循環参照パターン 1〜4 |
| Phase 6（プレースホルダ） | 全プレースホルダの展開、エスケープ `{{ }}`、未知プレースホルダのエラー |
| Phase 7（ファイル監視） | — （手動テストのみ。notify のイベント受信を確認） |
| Phase 8（ルーター） | デバウンス、target フィルタ、glob/regex マッチ、events 積集合 |
| Phase 9（コピー） | 正常コピー、上書きスキップ（WARN）、preserve_structure、リトライ、BLAKE3 ハッシュ検証（成功/失敗） |
| Phase 10（移動） | 正常移動、異ボリュームフォールバック、プレースホルダ更新、異ボリューム時のハッシュ検証・不一致時の元ファイル保護 |
| Phase 11（command/execute） | プロセス起動の成功確認（fire-and-forget） |

### 4.3 テストで使うテクニック

- **一時ディレクトリ**: `tempfile` クレートで毎テストごとにディレクトリを作り、終了時に自動削除
- **TOML 文字列**: テストコード内に TOML リテラルを直書きして `toml::from_str()` でパース
- **assert マクロ**: `assert_eq!`, `assert!(result.is_err())` が基本

---

## 5. フェーズ一覧（概要）

| Phase | 状態 | 名前 | 主な成果物 | 学ぶ概念 |
|-------|------|------|-----------|---------|
| **0** | ✅ 完了 | プロジェクト構築 | Cargo ワークスペース、ファイル骨格 | Cargo workspace, mod 宣言 |
| **1** | ✅ 完了 | Hello World 疎通 | `cargo run -p cat-watcher` で起動 | クレート分割, `cargo run -p` |
| **2** | ✅ 完了 | エラー型定義 | `error.rs` | `thiserror`, enum, `Result` |
| **3** | ✅ 完了 | 設定読み込み | `config.rs`（構造体 + デシリアライズ） | `serde`, `toml`, derive マクロ |
| **4** | ✅ 完了 | CLI | `main.rs`（clap 引数パース） | `clap` derive API |
| **5** | ✅ 完了 | バリデーション | `config.rs`（バリデーション関数群） | `Path`, `canonicalize`, グラフ探索 |
| **6** | ✅ 完了 | プレースホルダ | `placeholder.rs` | 文字列パーサ, `chrono` (or 標準 `time`) |
| **7** | 🔲 未着手 | ファイル監視（基本） | `watcher.rs` | `notify`, `tokio`, async/await |
| **8** | 🔲 未着手 | ルーター＋デバウンス | `router.rs` | `HashMap`, `HashSet`, `tokio::time`, `globset`, `regex` |
| **9** | 🔲 未着手 | アクション: copy | `actions/copy.rs`, `actions/mod.rs` | ファイル I/O, リトライ, `tokio::fs` |
| **10** | 🔲 未着手 | アクション: move | `actions/move_file.rs` | `rename`, cross-volume fallback |
| **11** | 🔲 未着手 | アクション: command / execute | `actions/command.rs`, `actions/execute.rs` | `tokio::process::Command`, Windows API |
| **12** | 🔲 未着手 | 統合・仕上げ | 起動シーケンス完成、初回スキャン、グレースフル停止、ログ | `tracing`, `tracing-appender`, `tokio::signal` |
| **13** | 🔲 未着手 | csv2toml 実装 | `csv2toml/` クレート | `csv` クレート, BOM 処理 |
| **14** | 🔲 未着手 | 総合テスト・設定サンプル | `config/` サンプル、`tool/` テストスクリプト | 実運用シナリオ検証 |

---

## 6. フェーズ詳細

---

### Phase 0 — プロジェクト構築

**ゴール**: Cargo ワークスペースのディレクトリ構成を作り、`cargo check` が通る状態にする。

#### やること

1. ルートの `Cargo.toml` をワークスペース定義に書き換える
2. `cat-watcher/` ディレクトリとその `Cargo.toml` を作成する
3. `csv2toml/` ディレクトリとその `Cargo.toml` を作成する（中身は空の `main.rs` のみ）
4. `cat-watcher/src/` 配下にモジュールの空ファイルを作成する
5. 既存の `src/main.rs` を削除する（`cat-watcher/src/main.rs` に移行）
6. `config/` ディレクトリにサンプル `global.toml` と `rules.toml` を作成する

#### 作成・変更ファイル

```
Cargo.toml                          # workspace定義に変更
cat-watcher/Cargo.toml              # 新規
cat-watcher/src/main.rs             # 新規（空のmain）
cat-watcher/src/config.rs           # 新規（空）
cat-watcher/src/watcher.rs          # 新規（空）
cat-watcher/src/router.rs           # 新規（空）
cat-watcher/src/placeholder.rs      # 新規（空）
cat-watcher/src/error.rs            # 新規（空）
cat-watcher/src/actions/mod.rs      # 新規（空）
cat-watcher/src/actions/copy.rs     # 新規（空）
cat-watcher/src/actions/move_file.rs # 新規（空）
cat-watcher/src/actions/command.rs  # 新規（空）
cat-watcher/src/actions/execute.rs  # 新規（空）
csv2toml/Cargo.toml                 # 新規
csv2toml/src/main.rs                # 新規（空のmain）
config/global.toml                  # 新規（サンプル）
config/rules.toml                   # 新規（サンプル）
```

#### 学ぶべき概念

- **Cargo ワークスペース**: ルートの `Cargo.toml` に `[workspace] members = [...]` を書く方法
- **mod 宣言**: `main.rs` から他モジュールを `mod config;` のように宣言する方法
- **`cargo check`**: コンパイルだけ行い実行しないコマンド

#### 完了条件

- [x] `cargo check` がエラーなしで通る
- [x] `cargo build` が成功する
- [x] `cat-watcher` と `csv2toml` 両方のバイナリが生成される

---

### Phase 1 — Hello World 疎通

**ゴール**: `cargo run -p cat-watcher` で最小限のメッセージが表示される。

#### やること

1. `cat-watcher/src/main.rs` に `fn main()` + 簡単な `println!` を書く
2. 各モジュールが `mod` 宣言でリンクされ、コンパイルが通ることを確認する

#### 学ぶべき概念

- `cargo run -p <パッケージ名>` で特定クレートを実行する方法
- モジュールツリー（`mod` / `pub`）

#### 完了条件

- [x] `cargo run -p cat-watcher` でメッセージが表示される
- [x] `cargo run -p csv2toml` でメッセージが表示される
- [x] 全モジュールが `mod` 宣言でつながり、`cargo check` が通る

---

### Phase 2 — エラー型定義

**ゴール**: アプリケーション全体で使うエラー型を定義し、`Result<T, AppError>` パターンを確立する。

#### やること

1. `cat-watcher/Cargo.toml` に `thiserror` を追加する
2. `error.rs` にエラー enum を定義する

#### 定義するエラー型（例）

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("設定ファイルエラー: {0}")]
    Config(String),

    #[error("バリデーションエラー: {0}")]
    Validation(String),

    #[error("I/O エラー: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML パースエラー: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("監視エラー: {0}")]
    Watch(String),

    #[error("アクション実行エラー: {0}")]
    Action(String),
}
```

#### 学ぶべき概念

- **`thiserror`**: derive マクロでエラー型を簡潔に定義する方法
- **`Result<T, E>`**: Rust のエラーハンドリングパターン
- **`?` 演算子**: エラーの伝播
- **`From` トレイト**: `#[from]` による自動変換

#### テスト

- エラー型の Display 出力が想定どおりか

#### 完了条件

- [x] `error.rs` にエラー型が定義されている
- [x] 他モジュールから `use crate::error::AppError;` でインポートできる
- [x] `cargo test -p cat-watcher` が通る

---

### Phase 3 — 設定読み込み

**ゴール**: `global.toml` と `rules.toml` を Rust の構造体にデシリアライズできる。

#### やること

1. `cat-watcher/Cargo.toml` に `serde`, `toml` を追加する
2. `config.rs` に設定構造体を定義する
3. TOML ファイルを読み込んでデシリアライズする関数を実装する

#### 定義する構造体

```
GlobalConfig
  └ global: GlobalSettings
      ├ log_level: String
      ├ log_file: String
      ├ log_rotation: String
      ├ retry_count: u32
      ├ retry_interval_ms: u64
      └ dry_run: bool

RulesConfig
  └ rules: Vec<Rule>
      ├ name: String
      ├ enabled: bool
      ├ watch: WatchConfig
      │   ├ path: String
      │   ├ recursive: bool
      │   ├ target: String
      │   ├ include_hidden: bool
      │   ├ patterns: Option<Vec<String>>      ← regex と排他
      │   ├ exclude_patterns: Option<Vec<String>>  ← 任意
      │   ├ regex: Option<String>               ← patterns と排他
      │   └ events: Vec<String>
      └ actions: Vec<ActionConfig>
          ├ type_: String                       ← "type" は予約語なので rename
          ├ destination: Option<String>
          ├ overwrite: Option<bool>
          ├ preserve_structure: Option<bool>
          ├ verify_integrity: Option<bool>
          ├ shell: Option<String>
          ├ command: Option<String>
          ├ program: Option<String>
          ├ args: Option<Vec<String>>
          └ working_dir: Option<String>
```

> **設計判断**: TOML の `type` は Rust の予約語と衝突するため `#[serde(rename = "type")]` を使う。  
> アクションのフィールドは type ごとに必要/不要が異なるため、`Option<T>` で受けてバリデーションで必須チェックする。

#### 学ぶべき概念

- **`serde::Deserialize`**: derive マクロによる自動デシリアライズ
- **`#[serde(rename = "...")]`**: フィールド名の変換
- **`#[serde(default)]`**: デフォルト値（exclude_patterns 用）
- **`Option<T>`**: Rust の「あるかもしれない値」
- **`std::fs::read_to_string()`**: ファイル読み込み

#### テスト

```
[正常系]
- global.toml の全項目が正しくパースされる
- rules.toml の copy ルールがパースされる
- rules.toml の command ルールがパースされる
- rules.toml の execute ルールがパースされる
- exclude_patterns 省略時に None になる
- 複数ルールがパースされる
- アクションチェーン（同一ルールに複数アクション）がパースされる

[異常系]
- 必須フィールドが欠けた TOML でエラーになる
- 型が合わない値（log_level に数値等）でエラーになる
```

#### 完了条件

- [ ] サンプル `global.toml` を読み込み、構造体にデシリアライズできる
- [ ] サンプル `rules.toml` を読み込み、構造体にデシリアライズできる
- [ ] ユニットテストが全件パスする

---

### Phase 4 — CLI

**ゴール**: コマンドライン引数をパースし、設定ファイルを読み込んで「設定を読み込みました」と表示する。

#### やること

1. `cat-watcher/Cargo.toml` に `clap` を追加する
2. `main.rs` に clap の derive API で CLI 引数の構造体を定義する
3. 引数に応じて設定ファイルを読み込む処理をつなぐ

#### CLI 引数構造体

```
struct Cli {
    global: PathBuf,     // -g / --global
    rules: PathBuf,      // -r / --rules
    dry_run: bool,       // --dry-run
    log_level: Option<String>,  // --log-level
    validate: bool,      // --validate
}
```

#### 学ぶべき概念

- **`clap` derive API**: `#[derive(Parser)]` によるコマンドライン引数の定義
- **`PathBuf`**: ファイルパスの型
- **`std::process::exit()`**: 終了コードの指定

#### 動作確認

```powershell
# ヘルプ表示
cargo run -p cat-watcher -- --help

# バリデーションモード
cargo run -p cat-watcher -- -g config/global.toml -r config/rules.toml --validate

# 存在しないファイルでエラーになることを確認
cargo run -p cat-watcher -- -g noexist.toml -r config/rules.toml
```

#### 完了条件

- [ ] `--help` でヘルプが表示される
- [ ] `--version` でバージョンが表示される
- [ ] `-g` と `-r` で設定ファイルを読み込み、内容を表示できる
- [ ] `--validate` で読み込み成功メッセージ → exit 0

---

### Phase 5 — バリデーション

**ゴール**: 詳細設計書 §3.4 の全バリデーションチェックを実装する。

#### やること

1. `config.rs` にバリデーション関数を追加する
2. 各チェックを個別関数として実装する

#### バリデーション項目（実装順）

**基本チェック（先に実装）**:
1. `events` が空配列でないこと
2. `actions` が空配列でないこと
3. `patterns` と `regex` の排他チェック（両方指定 or 両方なしでエラー）
4. `type` ごとの必須フィールド確認（`verify_integrity` 含む）
5. `target` の値が `file` / `directory` / `both` のいずれかであること
6. `log_level` の値が `trace` / `debug` / `info` / `warn` / `error` のいずれかであること
7. `log_rotation` の値が `daily` / `never` のいずれかであること

**パスチェック（次に実装）**:
8. `watch.path` が実在すること
9. `destination` が実在すること（`preserve_structure = true` の場合はルートのみ）
10. 同名ルールの watch 設定一致チェック

**高度なチェック（最後に実装）**:
11. glob / regex 構文チェック（コンパイルが成功すること）
12. プレースホルダ検証（未知のプレースホルダがないこと） — Phase 6 後に実装
13. 循環参照チェック（4 パターン）

#### 循環参照チェックの実装方針

```
パターン 1: destination == watch_path          → パスの一致比較
パターン 2: destination が watch_path の配下     → starts_with チェック（recursive=true 時）
パターン 3: watch_path が destination の配下     → starts_with チェック
パターン 4: ルール間の循環                      → 有向グラフ + DFS で閉路検出
```

パターン 4 の具体的アルゴリズム:
- 各ルールの `(watch_path, destination)` ペアを集める
- ルール A の destination がルール B の watch_path（またはその配下）に含まれる場合、A → B のエッジを張る
- このグラフに対して DFS で閉路を検出する

#### 学ぶべき概念

- **`std::path::Path::canonicalize()`**: パスの正規化
- **`Path::starts_with()`**: パスの包含関係の判定
- **`HashMap` / `Vec`**: グラフの表現
- **DFS（深さ優先探索）**: 閉路検出アルゴリズム

#### テスト

```
[基本チェック]
- events 空 → エラー
- patterns と regex 両方指定 → エラー
- patterns も regex も未指定 → エラー
- copy なのに destination 未指定 → エラー
- command なのに shell 未指定 → エラー

[循環参照]
- パターン 1: destination == watch_path → エラー
- パターン 2: destination が watch_path/sub → エラー（recursive=true）
- パターン 2: destination が watch_path/sub → OK（recursive=false）
- パターン 3: watch_path が destination/sub → エラー
- パターン 4: A→B→A の循環 → エラー
- 循環なし → OK
```

#### 完了条件

- [x] 全バリデーション項目が実装されている（パターン4を除く）
- [x] 不正な設定ファイルを食わせるとエラーメッセージ付きで exit 1 になる
- [x] `--validate` で正常設定ファイルを検証すると exit 0 になる
- [x] ユニットテストが全件パスする

#### ⚠️ 未実装事項

- 循環参照チェック **パターン4（ルール間循環）は未実装**。
  パスベースの静的チェックだけでは patterns/regex による実際の発火条件を考慮できず、
  正常な設定でも偽陽性のエラーになるリスクがあるため意図的にスキップ。
  パターン 1〜3（単一ルール内の same/sub/parent チェック）で実用上の主要ケースはカバー済み。

---

### Phase 6 — プレースホルダ

**ゴール**: 文字列中の `{Name}` 等を実際の値に展開できる。

#### やること

1. `placeholder.rs` にプレースホルダの解析・展開ロジックを実装する
2. バリデーション（Phase 5）にプレースホルダ検証を追加する

#### 実装する関数

```rust
/// 文字列中のプレースホルダを展開する
pub fn expand(template: &str, context: &PlaceholderContext) -> Result<String, AppError>

/// 文字列中のプレースホルダが既知のもののみであることを検証する
pub fn validate(template: &str) -> Result<(), AppError>
```

#### PlaceholderContext

```rust
pub struct PlaceholderContext {
    pub full_name: String,       // {FullName}
    pub directory_name: String,  // {DirectoryName}
    pub name: String,            // {Name}
    pub base_name: String,       // {BaseName}
    pub extension: String,       // {Extension}
    pub relative_path: String,   // {RelativePath}
    pub watch_path: String,      // {WatchPath}
    pub destination: String,     // {Destination}
    // {Date}, {Time}, {DateTime} は展開時に動的生成
}
```

#### パーサの実装方針

1. 文字列を先頭からスキャンする
2. `{{` → リテラル `{` を出力
3. `}}` → リテラル `}` を出力
4. `{` で始まり `}` で終わる部分 → プレースホルダ名を抽出 → コンテキストから値を取得
5. 一致しないプレースホルダ名 → バリデーション時はエラー

#### 学ぶべき概念

- **文字列のイテレーション**: `chars()`, `char_indices()`
- **`String` の構築**: `push_str()`, `push()`
- **日時の扱い**: `chrono::Local::now()` でローカル時刻を取得、`format!` でフォーマット

#### テスト

```
[展開テスト]
- "{Name}" → "report.csv"
- "{BaseName}.{Extension}" → "report.csv"
- "{{literal}}" → "{literal}"
- "{FullName}" → 絶対パス
- 拡張子なしファイルの {Extension} → ""
- {Date}, {Time}, {DateTime} のフォーマット確認

[バリデーション]
- "{Unknown}" → エラー
- "{Name}" → OK
- "text without placeholder" → OK
- "{{escaped}}" → OK
```

#### 完了条件

- [ ] 全プレースホルダの展開が動作する
- [ ] エスケープ `{{ }}` が動作する
- [ ] 未知のプレースホルダでバリデーションエラーになる
- [ ] Phase 5 のバリデーションにプレースホルダ検証が統合されている

---

### Phase 7 — ファイル監視（基本）

**ゴール**: `notify` クレートでディレクトリを監視し、ファイル操作を検知してコンソールに表示する。

#### やること

1. `cat-watcher/Cargo.toml` に `notify`, `tokio` を追加する
2. `main.rs` を `#[tokio::main]` の async main に変更する
3. `watcher.rs` に基本的な監視ロジックを実装する

#### 最小実装

```rust
// watcher.rs
pub async fn start_watching(paths: Vec<PathBuf>, recursive: bool) -> Result<(), AppError> {
    // 1. notify::recommended_watcher() で Watcher を作成
    // 2. watch_path を登録
    // 3. イベント受信ループ
    //    → とりあえず println! で表示
}
```

#### Watcher 統合

同一 `watch.path` を持つ複数ルールについて、OS レベルの watcher は 1 つにまとめる。

```rust
// watch_path → Vec<RuleIndex> のマッピングを構築
let mut watcher_map: HashMap<PathBuf, Vec<usize>> = HashMap::new();
```

#### 学ぶべき概念

- **`tokio`**: `#[tokio::main]`, `async fn`, `.await`
- **`notify` クレート**: `recommended_watcher()`, `Watcher` トレイト, `Event` 型
- **チャネル**: `std::sync::mpsc` or `tokio::sync::mpsc` によるスレッド間通信
- **`RecursiveMode`**: `Recursive` / `NonRecursive`

#### 動作確認

```powershell
# 1. ターミナル 1 で watcher を起動
cargo run -p cat-watcher -- -g config/global.toml -r config/rules.toml

# 2. ターミナル 2 で監視対象にファイルを作成
echo "test" > C:\data\incoming\test.txt

# 3. ターミナル 1 にイベントが表示されることを確認
```

#### 完了条件

- [ ] 監視対象ディレクトリにファイルを作成/変更/削除するとイベントが表示される
- [ ] 同一パスの複数ルールで watcher が重複しない
- [ ] `recursive = true` / `false` が正しく動作する

---

### Phase 8 — ルーター＋デバウンス

**ゴール**: イベントをデバウンスし、ルールのフィルタ条件でマッチング判定する。

#### やること

1. `cat-watcher/Cargo.toml` に `globset`, `regex` を追加する
2. `router.rs` にデバウンスとフィルタリングを実装する

#### デバウンス実装方針

```rust
// ファイルパス → (イベント種別の集合, 最後のイベント時刻) のマップ
let mut pending: HashMap<PathBuf, (HashSet<EventKind>, Instant)> = HashMap::new();

// 500ms タイマーで定期的にチェック
// Instant::elapsed() >= 500ms のエントリを取り出してルール評価へ
```

`tokio::time::interval(Duration::from_millis(100))` 程度でポーリングし、500ms 経過したエントリを処理する。

#### フィルタパイプライン

```
1. target フィルタ
   - ファイルかディレクトリかを判定（Path::is_file() / is_dir()）
   - delete イベント時は判定不能 → notify の EventKind から推定

2. include_hidden フィルタ （Phase 12 で Windows API 統合）
   - この段階ではスタブ（常に通過）で実装

3. patterns / exclude_patterns / regex マッチ
   - globset::GlobSet でコンパイル済みパターンとマッチ
   - regex::Regex でマッチ
   - マッチ対象はファイル名/ディレクトリ名（Name 部分）

4. events 積集合判定
   - デバウンス集約結果の HashSet と ルールの events の intersection
   - 空でなければマッチ
```

#### 学ぶべき概念

- **`HashMap`** / **`HashSet`**: コレクション操作
- **`tokio::time::interval()`**: 定期タイマー
- **`globset::GlobSet`**: 複数パターンの一括マッチ
- **`regex::Regex`**: 正規表現マッチ
- **`Instant`** / **`Duration`**: 時間の計測

#### テスト

```
[デバウンス]
- 500ms 以内の同一パスの複数イベントが 1 つに集約される
- 異なるパスのイベントはそれぞれ独立に処理される

[target フィルタ]
- target="file" でディレクトリイベントは除外される
- target="both" で両方通過する

[パターンマッチ]
- "*.csv" が "report.csv" にマッチする
- "*.csv" が "report.txt" にマッチしない
- exclude_patterns で除外される
- regex が正しくマッチする

[events 判定]
- 集約 {create, modify} と events ["create"] → マッチ
- 集約 {delete} と events ["create"] → 不一致
```

#### 完了条件

- [ ] デバウンスが 500ms で動作する
- [ ] フィルタパイプラインが正しく機能する
- [ ] マッチしたルールの情報が後段に渡せる
- [ ] ユニットテストが全件パスする

---

### Phase 9 — アクション: copy

**ゴール**: マッチしたファイルを指定先にコピーできる。

#### やること

1. `actions/mod.rs` にアクションチェーン実行の枠組みを作る
2. `actions/copy.rs` にコピーロジックを実装する

#### actions/mod.rs の設計

```rust
pub async fn execute_chain(
    actions: &[ActionConfig],
    context: &mut PlaceholderContext,
    global: &GlobalSettings,
    dry_run: bool,
) -> Result<(), AppError> {
    for action in actions {
        match action.type_.as_str() {
            "copy" => copy::execute(action, context, global, dry_run).await?,
            "move" => { /* Phase 10 */ },
            "command" => { /* Phase 11 */ },
            "execute" => { /* Phase 11 */ },
            _ => return Err(AppError::Action(format!("未知のアクション: {}", action.type_))),
        }
    }
    Ok(())
}
```

#### copy の実装

```
1. destination + (preserve_structure ? relative_path : name) → 宛先パス算出
2. 宛先ファイルが存在し overwrite=false → WARN ログ + スキップ（正常扱い）
3. 中間ディレクトリの自動作成（preserve_structure=true 時）
retry_remaining = retry_count
loop:
  4. tokio::fs::copy() でコピー実行
     → I/O エラー発生時は手順 7 へ
  5. verify_integrity=true の場合、ソースと宛先の BLAKE3 ハッシュ値を比較
     → 一致 → 成功（手順 9 へ）
     → 不一致 → 宛先ファイルを削除
  6. verify_integrity=false の場合 → 成功（手順 9 へ）
  7. retry_remaining > 0 → WARN ログ → retry_interval_ms 待機 → retry_remaining -= 1 → loop
  8. retry_remaining == 0 → ERROR ログ → 不正な宛先が残っていれば削除 → エラー
9. 成功後、context.destination を更新
```

#### フォルダコピー

`target = "directory"` の場合、ディレクトリの中身ごと再帰コピーする。
`walkdir` クレートまたは自前の再帰で実装。

#### 学ぶべき概念

- **`tokio::fs`**: 非同期ファイル操作（`copy`, `create_dir_all`）
- **リトライパターン**: ループ + `tokio::time::sleep()` + エラー判定
- **`blake3`**: ストリーミングハッシュ計算（`Hasher::new()` + `update()` + `finalize()`）
- **`Path::join()`**: パスの結合
- **`Path::exists()`**: ファイル存在確認

#### テスト（tempfile 使用）

```
- ファイルが正しくコピーされる
- preserve_structure=true でサブディレクトリ構造が維持される
- preserve_structure=false でフラット配置される
- overwrite=false で既存ファイルがスキップされ、チェーンが継続する
- overwrite=true で上書きされる
- コピー後に context.destination が更新される
- ディレクトリの再帰コピーが動作する
- verify_integrity=true でコピー後に BLAKE3 ハッシュ値が一致する
- verify_integrity=true でハッシュ不一致時に宛先が削除されリトライされる
- verify_integrity=true で最終失敗時に不正な宛先ファイルが削除される
- verify_integrity=false でハッシュ検証がスキップされる
```

#### 完了条件

- [ ] ファイルコピーが動作する
- [ ] フォルダコピーが動作する
- [ ] preserve_structure が正しく動作する
- [ ] overwrite=false のスキップが正常扱いで動く
- [ ] リトライが動作する
- [ ] dry_run=true でコピーしない（ログのみ）
- [ ] verify_integrity=true でハッシュ検証が成功する
- [ ] verify_integrity=true でハッシュ不一致時に宛先削除＋リトライが動作する
- [ ] verify_integrity=true で最終失敗時に不正な宛先が削除される
- [ ] verify_integrity=false でハッシュ検証がスキップされる

---

### Phase 10 — アクション: move

**ゴール**: ファイル/フォルダを移動できる。move 後にプレースホルダが更新される。

#### やること

1. `actions/move_file.rs` に移動ロジックを実装する
2. 異ボリュームフォールバック（copy → delete）を実装する
3. アクションチェーンのプレースホルダ更新を実装する

#### move の実装

```
1. destination パス算出（copy と同様のロジック）
2. overwrite / スキップ判定（copy と同様）
3. tokio::fs::rename() を試行
   → 成功 → 手順 11 へ（同一ボリューム、ハッシュ検証スキップ）
4. rename 失敗かつ cross-device → copy にフォールバック
retry_remaining = retry_count
loop:
  5. tokio::fs::copy() でコピー実行
     → I/O エラー発生時は手順 9 へ
  6. verify_integrity=true の場合、ソースと宛先の BLAKE3 ハッシュ比較
     → 一致 → 元ファイル削除 → 成功（手順 11 へ）
     → 不一致 → 宛先ファイルを削除
  7. verify_integrity=false の場合 → 元ファイル削除 → 成功（手順 11 へ）
  8. rename 失敗（cross-device 以外）のリトライも同様
  9. retry_remaining > 0 → WARN ログ → retry_interval_ms 待機 → retry_remaining -= 1 → loop
 10. retry_remaining == 0 → ERROR ログ → 不正な宛先が残っていれば削除
     → 元ファイルは保持（データ消失防止）→ エラー
11. 成功後、context の FullName 等を移動先に更新
12. context.destination を更新
```

#### 学ぶべき概念

- **`tokio::fs::rename()`**: ファイル移動/リネーム
- **`ErrorKind::CrossesDevices` 等**: 異ボリュームエラーの検出（OS 固有）
- **プレースホルダ更新**: move 後に `PlaceholderContext` のパス情報を書き換える

#### テスト

```
- 同一ボリューム内でファイルが移動される
- 移動後のプレースホルダ（FullName 等）が移動先に更新される
- ディレクトリ移動が動作する
- verify_integrity=true で異ボリュームフォールバック時にハッシュ検証が行われる
- verify_integrity=true でハッシュ不一致時に宛先が削除されリトライされる
- verify_integrity=true で最終失敗時に元ファイルが削除されない
- verify_integrity=true で最終失敗時に不正な宛先ファイルが削除される
- verify_integrity=true で同一ボリューム rename 時はハッシュ検証がスキップされる
```

#### 完了条件

- [ ] ファイル移動が動作する
- [ ] move 後のプレースホルダ更新が正しい
- [ ] アクションチェーンで copy → move 等の連携が動作する
- [ ] verify_integrity=true で異ボリュームフォールバック時にハッシュ検証が動作する
- [ ] ハッシュ不一致時に宛先削除＋リトライが動作する
- [ ] 最終失敗時に元ファイルが保護される
- [ ] 最終失敗時に不正な宛先ファイルが削除される

---

### Phase 11 — アクション: command / execute

**ゴール**: シェルコマンドと外部プロセスを fire-and-forget で起動できる。

#### やること

1. `actions/command.rs` — シェル経由のコマンド実行
2. `actions/execute.rs` — `CreateProcessW` 直接起動

#### command の実装

```rust
pub async fn execute(action: &ActionConfig, context: &PlaceholderContext, dry_run: bool) -> Result<(), AppError> {
    let expanded_command = placeholder::expand(&action.command.as_ref().unwrap(), context)?;
    let working_dir = if action.working_dir is empty { None } else { Some(&action.working_dir) };

    let mut cmd = match action.shell.as_deref() {
        Some("cmd") => {
            let mut c = tokio::process::Command::new("cmd.exe");
            c.args(["/C", &expanded_command]);
            c
        }
        Some("powershell") => {
            let mut c = tokio::process::Command::new("powershell.exe");
            c.args(["-NoProfile", "-Command", &expanded_command]);
            c
        }
        Some("pwsh") => {
            let mut c = tokio::process::Command::new("pwsh.exe");
            c.args(["-NoProfile", "-Command", &expanded_command]);
            c
        }
        _ => return Err(AppError::Action("不明なシェル".into())),
    };

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    // fire-and-forget: spawn してすぐ返す
    cmd.spawn()?;
    Ok(())
}
```

#### execute の実装

- `tokio::process::Command` で `program` を直接起動する
- 引数 `args` 内のプレースホルダを展開する
- fire-and-forget

> **注**: 詳細設計書は `CreateProcessW` 直接起動と記載しているが、`tokio::process::Command` は内部で `CreateProcessW` を使用しているため、まずは `tokio::process::Command` で実装し、必要に応じて Windows API 直接呼び出しに切り替える。

#### 学ぶべき概念

- **`tokio::process::Command`**: 非同期プロセス起動
- **`spawn()`**: プロセスを起動して制御を返す（fire-and-forget）
- **`.current_dir()`**: 作業ディレクトリの設定

#### テスト

```
- command: cmd.exe /C echo test が起動する
- execute: 指定プログラムが起動する
- プレースホルダが展開された状態でコマンドが実行される
- working_dir が設定される
- dry_run 時はプロセスが起動しない
```

#### 完了条件

- [ ] command（cmd / powershell / pwsh）が動作する
- [ ] execute が動作する
- [ ] fire-and-forget で即座に次のアクションに進む
- [ ] working_dir が空文字列のとき watcher の CWD が使われる

---

### Phase 12 — 統合・仕上げ

**ゴール**: 起動シーケンスを完成させ、ログ出力、初回スキャン、グレースフルシャットダウン、隠しファイルフィルタを実装する。全体を通しで動かせる状態にする。

#### やること（サブタスク）

##### 12-A: ログ出力

1. `cat-watcher/Cargo.toml` に `tracing`, `tracing-subscriber`, `tracing-appender` を追加
2. JSON 構造化ログのセットアップ
3. ログローテーション（daily / never）
4. 各モジュールのログ出力を仕込む

##### 12-B: 隠しファイルフィルタ

1. `windows` クレート（または `winapi`）を追加
2. `GetFileAttributesW` で `FILE_ATTRIBUTE_HIDDEN` を判定
3. router.rs の include_hidden フィルタのスタブを実装に置き換える
4. 属性取得失敗時は処理対象とする（安全側）

##### 12-C: 初回スキャン

1. 起動シーケンスのステップ 8 を実装
2. `walkdir` や `std::fs::read_dir` で既存ファイルを列挙
3. events フィルタは適用しない（全マッチ対象をアクション実行）
4. target / include_hidden / patterns フィルタは適用する

##### 12-D: グレースフルシャットダウン

1. `tokio::signal::ctrl_c()` でシグナルハンドリング
2. shutdown フラグを `tokio::sync::watch` チャネルで全タスクに伝播
3. 実行中 copy/move の完了を待機
4. ログフラッシュ
5. exit 0

##### 12-E: watch_path 消失

1. notify のエラーイベントを検知
2. watch_path の消失を判定
3. ログ出力 → exit 2

##### 12-F: Rescan 対応

1. notify の `Rescan` イベントを受信
2. 対象ディレクトリのフルスキャンを実行

#### 学ぶべき概念

- **`tracing`**: 構造化ログ、`#[instrument]`, span
- **`tracing-appender`**: ファイル出力、ローテーション
- **`windows` クレート**: Windows API の安全な呼び出し
- **`tokio::signal::ctrl_c()`**: 非同期シグナルハンドリング
- **`tokio::sync::watch`**: シャットダウン通知パターン

#### 完了条件

- [ ] JSON ログがファイルに出力される
- [ ] ログローテーション（daily）が動作する
- [ ] 隠しファイルが正しくフィルタされる
- [ ] 起動時に既存ファイルが処理される
- [ ] Ctrl+C でグレースフルシャットダウンが動作する
- [ ] watch_path 消失で exit 2 になる
- [ ] **エンドツーエンドで、設定読み込み → 監視 → 検知 → アクション実行の全フローが動作する**

---

### Phase 13 — csv2toml 実装

**ゴール**: CSV ファイルを読み込み、rules.toml に変換する。

#### やること

1. `csv2toml/Cargo.toml` に `csv`, `serde`, `toml`, `clap` を追加
2. CLI 引数の定義
3. CSV パース → 中間構造体 → TOML 出力
4. バリデーション
5. BOM 処理

#### 実装の流れ

```
1. CSV 読み込み（BOM スキップ）
2. ヘッダ名から列を特定
3. 各行をパース → 中間構造体
4. 同一 name の行をグループ化（アクションチェーン統合）
5. バリデーション（§15.3）
6. TOML 構造体に変換
7. toml::to_string_pretty() で出力
8. ファイル書き出し or stdout 表示（dry-run）
```

#### 学ぶべき概念

- **`csv` クレート**: `Reader`, `StringRecord`, ヘッダ操作
- **BOM 処理**: ファイル先頭の 3 バイト `\xEF\xBB\xBF` をスキップ
- **`toml::to_string_pretty()`**: TOML のシリアライズ

#### 完了条件

- [ ] サンプル CSV から正しい rules.toml が生成される
- [ ] アクションチェーン（同名複数行）が正しく統合される
- [ ] `--validate` でバリデーション結果が表示される
- [ ] `--dry-run` で stdout に TOML が出力される
- [ ] BOM 付き UTF-8 CSV が読める

---

### Phase 14 — 総合テスト・設定サンプル

**ゴール**: 実運用に近いシナリオで全体テストし、設定サンプルとドキュメントを整備する。

#### やること

1. `config/` に運用仕様書 §9 の設定例を全パターン作成する
2. `tool/` にテスト用ファイル生成スクリプトを整備する
3. 統合テストシナリオを実施する

#### テストシナリオ

| # | シナリオ | 確認内容 |
|---|---------|---------|
| 1 | CSV 検知 → バックアップコピー | copy + preserve_structure |
| 2 | 画像検知 → コマンド + 外部プロセス | アクションチェーン（command + execute） |
| 3 | フォルダ作成 → まるごとコピー | target=directory |
| 4 | ログファイル → 移動 + コマンド | move + command チェーン |
| 5 | パターン不一致 → 隔離 | exclude_patterns |
| 6 | 隠しファイル除外 | include_hidden=false |
| 7 | 大量ファイル一括作成 | デバウンス・パフォーマンス |
| 8 | 再起動後の既存ファイル処理 | 初回スキャン |
| 9 | 監視ディレクトリ削除 | exit 2 |
| 10 | 循環参照設定 | バリデーションエラー |

#### 完了条件

- [ ] 全シナリオが正常に動作する
- [ ] `config/` にサンプル設定が揃っている
- [ ] `cargo clippy -- -D warnings` が通る
- [ ] `cargo fmt --check` が通る

---

## 7. フェーズ間の依存関係

```
Phase 0  プロジェクト構築
  │
Phase 1  Hello World
  │
  ├── Phase 2  エラー型 ──────────────────────┐
  │     │                                     │
  │   Phase 3  設定読み込み                    │
  │     │                                     │
  │   Phase 4  CLI                            │
  │     │                                     │
  │   Phase 5  バリデーション ←───────── Phase 6  プレースホルダ
  │     │                                     │
  │   Phase 7  ファイル監視（基本）            │
  │     │                                     │
  │   Phase 8  ルーター＋デバウンス            │
  │     │                                     │
  │     ├── Phase 9   copy ←──────────────────┤
  │     ├── Phase 10  move ←──────────────────┤
  │     └── Phase 11  command / execute ←─────┘
  │           │
  │         Phase 12  統合・仕上げ
  │           │
  │         Phase 13  csv2toml
  │           │
  └───────  Phase 14  総合テスト
```

**クリティカルパス**: 0 → 1 → 2 → 3 → 4 → 5 → 7 → 8 → 9 → 12

**並行実装可能**:
- Phase 6（プレースホルダ）は Phase 3 の後であればいつでも着手可
- Phase 10, 11 は Phase 9 の次に順番に進めるが、ロジックは独立

---

## 付録 A: 各フェーズで追加するクレート

| Phase | 追加クレート | Cargo.toml への記載 |
|-------|------------|-------------------|
| 2 | thiserror | `thiserror = "2"` |
| 3 | serde, toml | `serde = { version = "1", features = ["derive"] }`, `toml = "0.8"` |
| 4 | clap | `clap = { version = "4", features = ["derive"] }` |
| 6 | chrono | `chrono = "0.4"` |
| 7 | notify, tokio | `notify = "7"`, `tokio = { version = "1", features = ["full"] }` |
| 8 | globset, regex | `globset = "0.4"`, `regex = "1"` |
| 9 | tempfile (dev) | `[dev-dependencies] tempfile = "3"` |
| 12 | tracing, tracing-subscriber, tracing-appender, windows | `tracing = "0.1"`, `tracing-subscriber = { version = "0.3", features = ["json"] }`, `tracing-appender = "0.2"`, `windows = { version = "0.58", features = ["Win32_Storage_FileSystem"] }` |
| 13 | csv | `csv = "1"` |

> **注意**: バージョンは執筆時点の推奨。実装時に `cargo add <クレート名>` で最新安定版を追加すること。

---

## 付録 B: 推奨する学習順序

Phase を進める中で自然に学べるが、事前に目を通しておくと効率が上がるトピック:

1. **所有権・借用・ライフタイム** — Rust 最大の壁。Phase 2〜3 で直面する
2. **`Result` と `?` 演算子** — Phase 2 で集中的に学ぶ
3. **derive マクロ** — Phase 3（serde）、Phase 4（clap）で多用する
4. **`async` / `.await`** — Phase 7 で初めて使う。事前に tokio のチュートリアルを 1 つやっておくとよい
5. **トレイト** — Phase 9 のアクション抽象化で使う可能性がある
6. **Windows API 呼び出し** — Phase 12 で `windows` クレートを使う

---

## 付録 C: 設計書へのフィードバック（反映推奨）

§2 で確定した設計補足事項を詳細設計書に反映することを推奨する:

1. §3.3 の表: `exclude_patterns` の必須列を「○」に変更し、「空配列 `[]` で除外なしを明示」と明記
2. §3.3 の表: `working_dir` に「必須（command / execute 時）。空文字列で CWD」と明記
3. §5.3 に「events フィルタは初回スキャンには適用しない」と明記
4. §10.1 / §10.2 に「overwrite=false でスキップ時は正常扱い（WARN ログ出力、チェーン継続）」と明記
5. §8.3 に「patterns / regex / exclude_patterns のマッチ対象は target で指定された種類の名前部分」と明記
