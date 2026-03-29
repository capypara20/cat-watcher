# ファイル常駐監視アプリケーション 設計メモ

## 概要

特定フォルダをリアルタイム監視し、命名規則に合致するファイル（およびフォルダ）を検知して、コピー・コマンド実行・外部プロセス起動などのアクションを自動実行する **Windows 向け**常駐アプリケーション。

### 前提・方針

- **ターゲット OS**: Windows（`ReadDirectoryChangesW` を主軸）。クロスプラットフォーム対応は将来の拡張とする
- **設定管理フロー**: 運用者が **Excel で CSV を管理** → **CSV→TOML 変換ツール**で TOML に変換 → Rust アプリが TOML を読む
- **監視対象**: ファイルだけでなく **フォルダ（ディレクトリ）の出現** も検知対象
- **不一致処理**: パターンに合致しないファイル/フォルダに対しても別アクションを定義可能

---

## 1. 必要な機能一覧

### 1.1 コア機能

| 機能 | 説明 |
|------|------|
| **フォルダ監視** | 指定ディレクトリのファイルシステムイベント（作成・変更・削除・リネーム）をリアルタイム検知 |
| **再帰監視** | サブディレクトリ内にもパターン合致ファイルがあるかチェック（ON/OFF 切替可能）。監視ディレクトリ配下に新たに作られたサブディレクトリも自動的に監視対象に加わる |
| **命名規則フィルタ** | glob パターンや正規表現でファイル名をフィルタリング |
| **フォルダ検知** | 監視対象にフォルダが出現した場合もイベントとして検知し、専用アクションを実行可能 |
| **不一致ファイル処理** | パターンに合致 **しない** ファイル/フォルダに対する fallback アクション（ログ記録・移動・通知等）を定義可能 |
| **複数フォルダ監視** | 複数の監視対象フォルダをそれぞれ独立して設定可能 |

### 1.2 アクション機能

| 機能 | 説明 |
|------|------|
| **ファイルコピー** | 検知ファイルを指定先ディレクトリへコピー（上書き/スキップ選択） |
| **ファイル移動** | 検知ファイルを指定先へ移動 |
| **コマンド実行** | 任意のシェルコマンドを実行（検知ファイルパスをプレースホルダで渡せる） |
| **外部プロセス起動** | 指定した実行ファイル（.exe等）を引数付きで起動 |
| **アクションチェーン** | 1つのイベントに対して複数アクションを順次実行 |

### 1.3 制御・運用機能

| 機能 | 説明 |
|------|------|
| ~~**設定ファイルホットリロード**~~ | ~~アプリ再起動なしで設定変更を反映~~ → **不採用**。設定変更時はアプリを再起動する（§13.4 参照） |
| **ロギング** | イベント検知・アクション結果をログファイルに記録 |
| **デバウンス** | 短時間に連続するイベントをまとめて処理（内部固定ロジック。§14 参照） |
| **リトライ** | アクション失敗時のリトライ（回数・間隔を `global.toml` で設定。§16.6 参照） |
| **グレースフルシャットダウン** | シグナル（Ctrl+C 等）を受けて安全に終了（§13.2 参照） |
| **Windows サービス化** | バックグラウンド常駐（オプション） |
| **CSV→TOML 変換ツール** | Excel で管理する CSV ルール定義を TOML 設定ファイルに変換する補助ツール |

### 1.4 あると便利な機能（拡張）

| 機能 | 説明 |
|------|------|
| **ドライラン** | 実際のアクションを実行せずログだけ出力するモード |
| **条件分岐** | ファイルサイズ・拡張子・更新日時などによるアクション分岐 |
| **通知** | デスクトップ通知やメール通知 |
| **ステータス表示** | 現在の監視状態・統計情報の表示（CLI / HTTP） |

---

## 2. 設定ファイル構成

TOML 形式を推奨（Rust エコシステムとの相性が良い）。

> **運用フロー**: Excel(CSV) → `csv2toml` 変換ツール → `rules.toml` → Rust 監視アプリ
>
> 運用者は Excel で CSV を編集し、変換ツールを実行するだけで設定が反映される。
>
> **確定仕様**: 設定ファイルは `global.toml`（手動編集）と `rules.toml`（CSV変換）の2ファイル構成。全項目にデフォルト値なし（明示的に指定必須）。詳細は §11 参照。

### 2.1 設定ファイル例：`config.toml`

```toml
# ==============================================================================
# グローバル設定
# ==============================================================================
[global]
log_level = "info"                  # trace, debug, info, warn, error
log_file = "./logs/watcher.log"     # ログ出力先
log_rotation = "daily"              # daily, size(10MB), never
debounce_ms = 300                   # イベントデバウンス（ミリ秒）
dry_run = false                     # ドライランモード

# ==============================================================================
# 監視ルール（複数定義可能）
# ==============================================================================

# --- ルール1: CSVファイルを検知してコピー ---
[[rules]]
name = "csv-backup"
enabled = true

[rules.watch]
path = "C:/data/incoming"           # 監視対象ディレクトリ
recursive = true                    # サブディレクトリ内もパターンチェック
target = "file"                     # file | directory | both
patterns = ["*.csv", "report_*.xlsx"]  # glob パターン（複数指定可）
# regex = "^\\d{8}_report\\.csv$"   # 正規表現でも指定可（patterns と排他）
events = ["create", "modify"]       # 検知するイベント種別

[[rules.actions]]
type = "copy"
destination = "C:/data/backup"
overwrite = false                   # 既存ファイルの上書き
preserve_structure = true           # サブディレクトリ構造を維持

# --- ルール2: 画像ファイルを検知してコマンド実行 ---
[[rules]]
name = "image-process"
enabled = true

[rules.watch]
path = "C:/data/images"
recursive = false
patterns = ["*.png", "*.jpg"]
events = ["create"]

[[rules.actions]]
type = "command"
command = "magick convert {file} -resize 50% {dir}/thumb_{name}"
working_dir = "C:/data/images"
# プレースホルダ:
#   {file}  - フルパス
#   {dir}   - ディレクトリ
#   {name}  - ファイル名（拡張子含む）
#   {stem}  - ファイル名（拡張子なし）
#   {ext}   - 拡張子

[[rules.actions]]
type = "execute"
program = "C:/tools/uploader.exe"
args = ["--input", "{file}", "--mode", "auto"]

# --- ルール3: ログファイルを検知して移動 ---
[[rules]]
name = "log-archive"
enabled = true

[rules.watch]
path = "C:/app/logs"
recursive = false
target = "file"
patterns = ["*.log"]
events = ["create"]

[[rules.actions]]
type = "move"
destination = "C:/archive/logs"
overwrite = false

[[rules.actions]]
type = "command"
command = "echo Archived: {name} >> C:/archive/archive.log"

# --- ルール4: フォルダが来たらまるごとコピー ---
[[rules]]
name = "folder-intake"
enabled = true

[rules.watch]
path = "C:/data/incoming"
recursive = false
target = "directory"                # ディレクトリのみ検知
patterns = ["batch_*"]
events = ["create"]

[[rules.actions]]
type = "copy"
destination = "C:/data/processed"
overwrite = false

# --- ルール5: パターン不一致のファイルを隔離 ---
[[rules]]
name = "unmatched-quarantine"
enabled = true

[rules.watch]
path = "C:/data/incoming"
recursive = false
target = "file"
patterns = ["*"]                    # 全ファイルにマッチ
exclude_patterns = ["*.csv", "*.xlsx", "*.log"]  # 除外パターン（=正規ルールでカバー済み）
events = ["create"]

[[rules.actions]]
type = "move"
destination = "C:/data/quarantine"

[[rules.actions]]
type = "command"
command = "echo [WARN] Unexpected file: {name} >> C:/data/quarantine/quarantine.log"
```

### 2.2 設定項目の一覧

| セクション | キー | 型 | 説明 |
|-----------|------|-----|------|
| `global` | `log_level` | string | ログレベル |
| `global` | `log_file` | string | ログファイルパス |
| `global` | `log_rotation` | string | ログローテーション方式。daily: 日次でローテーション / size(10MB): 指定サイズ超過でローテーション / never: ローテーションしない |
| `global` | `debounce_ms` | u64 | デバウンス時間（ms） |
| `global` | `dry_run` | bool | ドライランモード |
| `rules[].watch` | `path` | string | 監視対象パス |
| `rules[].watch` | `recursive` | bool | サブディレクトリ内もパターンチェックするか |
| `rules[].watch` | `target` | string | 検知対象: `file` / `directory` / `both` |
| `rules[].watch` | `include_hidden` | bool | 隠しファイル・隠しフォルダを検知対象に含めるか（`true`: 含める / `false`: 除外） |
| `rules[].watch` | `patterns` | string[] | glob パターン |
| `rules[].watch` | `exclude_patterns` | string[] | 除外 glob パターン（不一致ファイル処理用） |
| `rules[].watch` | `regex` | string | 正規表現パターン |
| `rules[].watch` | `events` | string[] | 検知イベント種別。指定可能な値: create（作成）/ modify（変更）/ delete（削除）/ rename（リネーム・移動） |
| `rules[].actions[]` | `type` | string | アクション種別（copy/move/command/execute） |
| `rules[].actions[]` | `destination` | string | コピー/移動先 |
| `rules[].actions[]` | `overwrite` | bool | 宛先に同名ファイルが既に存在する場合に上書きするか（true: 上書き / false: スキップ）。action type が copy または move の場合は必須。command / execute では使用しない |
| `rules[].actions[]` | `preserve_structure` | bool | サブディレクトリ構造の維持（詳細はセクション8.3参照）。action type が copy または move かつ recursive = true の場合に有効 |
| `rules[].actions[]` | `command` | string | 実行コマンド |
| `rules[].actions[]` | `program` | string | 実行ファイルパス |
| `rules[].actions[]` | `args` | string[] | 引数リスト |
| `rules[].actions[]` | `working_dir` | string | command / execute で起動するプロセスのカレントディレクトリ。コマンド内で相対パスを使用する場合や、プロセスが相対パスでファイルを出力する場合にのみ指定する。絶対パスを使用するなら不要 |

---

## 3. パフォーマンス考慮事項

### 3.1 ファイル監視の仕組み

Rust では **`notify`** クレートが `ReadDirectoryChangesW` をラップしており、Windows 上で高性能な監視が可能。

> **Windows 固有の注意点**:
> - `ReadDirectoryChangesW` はディレクトリ単位でハンドルを取る方式なので、再帰監視でもハンドル消費は少ない
> - 対象はローカルドライブおよび SMB ファイルサーバー。SMB で OS 通知が届かない環境は動作保証外とする
> - NTFS ジャーナル変更通知はサポート外 → `notify` のイベントストリームで十分

### 3.2 パフォーマンスのポイント

#### イベント処理

- **デバウンス必須**: エディタやツールがファイルを保存すると、短時間に複数イベント（create→modify→modify 等）が発生する。300ms 程度のデバウンスで集約する。
- **非同期処理**: アクション（特にコマンド実行・外部プロセス）は非同期で実行し、監視スレッドをブロックしない。`tokio` を使った async/await が適切。
- **バッファサイズ**: `notify` のイベントバッファが溢れるとイベントをロストする。大量ファイルが同時に作成されるケースでは、チャネルバッファサイズを十分に確保する。

#### メモリ・CPU

- **アイドル時のリソース消費**: OS ネイティブ API 方式ならアイドル時の CPU 使用率はほぼ 0%。
- **監視対象数の上限**: Windows の `ReadDirectoryChangesW` はディレクトリ単位で監視するため、再帰監視でもハンドル数は少ない。数百フォルダ規模なら問題なし。
- **正規表現のコンパイル**: 正規表現は起動時・設定リロード時に1回だけコンパイルし、使い回す（`regex::Regex` はコンパイル済みオブジェクト）。

#### 信頼性

- **イベントロスト対策**: OS バッファ溢れ時に `notify` は `Rescan` イベントを発行する。これを受けたら対象ディレクトリをフルスキャンして差分を検出する。
- **ファイルロック**: 検知直後はファイルが書き込み中の可能性がある。コピー/移動前に短いリトライ付きでファイルオープンを試みる。
- **ディスクI/O**: 大量ファイルコピーを並行実行するとI/Oがボトルネックになる。同時実行数を制限する（セマフォ）。

### 3.3 パフォーマンス目標の目安

| 指標 | 目標値 |
|------|--------|
| アイドル時 CPU | < 0.1% |
| アイドル時メモリ | < 10 MB |
| イベント検知遅延 | < 500ms（デバウンス含む） |


---

## 4. 推奨クレート構成

| 用途 | クレート | 備考 |
|------|---------|------|
| ファイル監視 | `notify` | Windows: `ReadDirectoryChangesW` ベース |
| 非同期ランタイム | `tokio` | async/await |
| 設定ファイル | `toml` + `serde` | TOML パース＆デシリアライズ |
| glob マッチ | `globset` | 高速 glob パターンマッチ |
| 正規表現 | `regex` | 標準的な正規表現 |
| ロギング | `tracing` + `tracing-subscriber` | 構造化ログ |
| ログローテーション | `tracing-appender` | ファイルローテーション |
| CLI 引数 | `clap` | コマンドライン引数パーサ |
| シグナルハンドリング | `tokio::signal` | Ctrl+C 等 |
| Windows サービス | `windows-service`（オプション） | サービス化する場合 |
| CSV パース | `csv` + `serde` | CSV→TOML 変換ツール用 |

---

## 5. アーキテクチャ概要

```
┌─────────────────────────────────────────────────┐
│                  設定ファイル (config.toml)         │
│                       │                           │
│                       ▼                           │
│              ┌──────────────┐                     │
│              │ Config Loader │◄── ホットリロード    │
│              └──────┬───────┘                     │
│                     │                             │
│                     ▼                             │
│  ┌────────────────────────────────────┐           │
│  │         Watcher (notify)           │           │
│  │  OS ネイティブ API でイベント検知    │           │
│  └────────────┬───────────────────────┘           │
│               │ イベント                           │
│               ▼                                   │
│  ┌────────────────────────────────────┐           │
│  │       Event Router                 │           │
│  │  デバウンス → フィルタ → ルール照合   │           │
│  └────────────┬───────────────────────┘           │
│               │ マッチしたルール + ファイル情報       │
│               ▼                                   │
│  ┌────────────────────────────────────┐           │
│  │       Action Executor (async)      │           │
│  │  ┌──────┐ ┌──────┐ ┌───────┐ ┌─────────┐ │
│  │  │ Copy │ │ Move │ │Command│ │ Execute │ │
│  │  └──────┘ └──────┘ └───────┘ └─────────┘ │
│  └────────────────────────────────────┘           │
│               │                                   │
│               ▼                                   │
│  ┌────────────────────────────────────┐           │
│  │       Logger (tracing)             │           │
│  └────────────────────────────────────┘           │
└─────────────────────────────────────────────────┘
```

---

## 6. プロジェクト構成案

```
src/
├── main.rs              # エントリポイント、シグナルハンドリング
├── config.rs            # 設定ファイルの読み込み・パース・バリデーション
├── watcher.rs           # ファイル監視のセットアップと制御
├── router.rs            # イベントのフィルタリング・デバウンス・ルート振り分け
├── actions/
│   ├── mod.rs           # Action トレイト定義
│   ├── copy.rs          # コピーアクション
│   ├── move_file.rs     # 移動アクション
│   ├── command.rs       # コマンド実行アクション
│   └── execute.rs       # 外部プロセス起動アクション
├── placeholder.rs       # プレースホルダ展開（{file}, {name} 等）
└── error.rs             # エラー型定義
```

---

## 7. セキュリティ考慮

> **確定仕様**: 詳細は §19 参照。

- **コマンドインジェクション**: サニタイズは実装しない。設定ファイルの NTFS 権限管理で防御する。外部ユーザーがファイル名を制御できる環境では `execute` タイプ（シェルを介さない直接起動）の使用を推奨
- **パストラバーサル防止**: コピー・移動先パスを正規化し、意図しないディレクトリへのアクセスを防ぐ
- **設定ファイルのアクセス制限**: 任意コマンドを実行可能なため、設定ファイル自体の権限管理が重要
- **外部プロセス**: command / execute で起動したプロセスは fire-and-forget（起動後に切り離し）。プロセスの終了監視はスコープ外
- **循環参照検知**: 起動時に watch_path と destination の循環をバリデーション（§19.1 参照）
- **シンボリックリンク**: 追跡しない（§19.2 参照）

---

## 8. CSV→TOML 変換ツール設計

### 8.1 概要

運用現場では Excel で監視ルールを管理するケースが多い。  
Excel で CSV をエクスポートし、変換ツールで TOML に変換するワークフローを提供する。

```
[Excel] → (CSV 保存) → [csv2toml ツール] → config.toml → [監視アプリ]
```

### 8.2 CSV フォーマット定義

#### `rules.csv`（メインルール定義）

| 列名 | 型 | 必須 | 説明 |
|------|----|------|------|
| `name` | string | ○ | ルール名（一意） |
| `enabled` | bool | ○ | 有効/無効 |
| `watch_path` | string | ○ | 監視対象ディレクトリ |
| `recursive` | bool | ○ | サブディレクトリ監視 |
| `target` | string | ○ | 検知対象: file/directory/both |
| `include_hidden` | bool | ○ | 隠しファイル・隠しフォルダを含めるか |
| `patterns` | string | ○ | glob パターン（`\|` 区切りで複数） |
| `exclude_patterns` | string | | 除外パターン（`\|` 区切り） |
| `regex` | string | | 正規表現（patterns と排他） |
| `events` | string | ○ | イベント種別（`\|` 区切り）。指定可能な値: create / modify / delete / rename（詳細はセクション2.2参照） |
| `action_type` | string | ○ | copy/move/command/execute |
| `action_destination` | string | | コピー/移動先。copy / move 時は必須 |
| `action_overwrite` | bool | | 上書き許可。copy / move 時は必須（詳細はセクション2.2参照） |
| `action_preserve_structure` | bool | | 構造維持。copy / move かつ recursive = true で有効（詳細はセクション8.3参照） |
| `action_command` | string | | 実行コマンド |
| `action_program` | string | | 実行ファイル |
| `action_args` | string | | 引数（`\|` 区切り） |
| `action_working_dir` | string | | 作業ディレクトリ。command / execute 専用（詳細はセクション2.2参照） |


#### CSV 例

```csv
name,enabled,watch_path,recursive,target,patterns,exclude_patterns,regex,events,action_type,action_destination,action_overwrite,action_preserve_structure,action_command,action_program,action_args,action_working_dir
csv-backup,true,C:/data/incoming,true,file,*.csv|report_*.xlsx,,, create|modify,copy,C:/data/backup,false,true,,,,
image-process,true,C:/data/images,false,file,*.png|*.jpg,,,create,command,,,,magick convert {file} -resize 50% {dir}/thumb_{name},,,C:/data/images
image-process,true,C:/data/images,false,file,*.png|*.jpg,,,create,execute,,,,, C:/tools/uploader.exe,--input|{file}|--mode|auto,
folder-intake,true,C:/data/incoming,false,directory,batch_*,,,create,copy,C:/data/processed,false,,,,,
unmatched,true,C:/data/incoming,false,file,*,*.csv|*.xlsx|*.log,,create,move,C:/data/quarantine,,,,,, 
```

> **複数アクション**: 同じ `name` の行を複数書くことでアクションチェーンを表現する。<br>
CSV例の`name`の**image-process**はチェーン上になっており上から順に実行される

### 8.3 `action_preserve_structure` の仕様

`recursive = true` の場合に、監視ルート配下のサブディレクトリ構造をコピー/移動先で再現するかを制御する。

#### `action_preserve_structure = false`（フラット）

検知されたファイルをサブディレクトリ構造を無視して宛先直下に配置する。

```
【監視ルート: C:/data/incoming】        【宛先: C:/data/backup】
├── report_A.csv           ──────►  ├── report_A.csv
├── sub1/                           ├── report_B.csv
│   └── report_B.csv       ──────►  └── report_C.csv
└── sub2/
    └── deep/
        └── report_C.csv   ──────►  （sub1/, sub2/ は作られない）
```

> サブディレクトリが異なっても同名ファイルがある場合は `action_overwrite` の設定に従う。

#### `action_preserve_structure = true`（構造維持）

監視ルートからの相対パス構造をそのまま宛先に再現する。

```
【監視ルート: C:/data/incoming】        【宛先: C:/data/backup】
├── report_A.csv           ──────►  ├── report_A.csv
├── sub1/                           ├── sub1/
│   └── report_B.csv       ──────►  │   └── report_B.csv
└── sub2/                           └── sub2/
    └── deep/                            └── deep/
        └── report_C.csv   ──────►           └── report_C.csv
```

> 対応する中間ディレクトリは自動的に作成される。

#### 適用条件

| 条件 | 挙動 |
|------|------|
| `recursive = false` | 監視ルート直下のみ対象なのでサブディレクトリ構造は発生しない。この設定は無効 |
| `action_type = command / execute` | ファイルを移動・コピーしないため、この設定は無効 |

---

### 8.4 変換ツールの仕様

```
csv2toml.exe --input rules.csv --output config.toml [--global global.toml]
```

| オプション | 説明 |
|-----------|------|
| `--input` | 入力 CSV ファイルパス |
| `--output` | 出力 TOML ファイルパス |
| `--global` | グローバル設定の TOML テンプレート（省略時はデフォルト値を使用） |
| `--validate` | TOML 生成後にバリデーションだけ実行 |
| `--dry-run` | 変換結果を stdout に出力（ファイル書き出しなし） |

### 8.5 変換時のバリデーション

- `name` の重複チェック（同名は複数アクションとして統合）
- `watch_path` の存在チェック（警告レベル）
- `patterns` と `regex` の排他チェック
- `action_type` に応じた必須フィールドの確認
- パスの正規化（`\` → `/` 統一） 

### 8.6 実装方針

変換ツールも Rust で実装し、同一リポジトリ内に **ワークスペースメンバー** として配置する：

```
Cargo.toml              # ワークスペース定義
watcher/                # 監視アプリ本体
│   ├── Cargo.toml
│   └── src/
csv2toml/               # CSV→TOML 変換ツール
│   ├── Cargo.toml
│   └── src/
config/                 # 設定ファイルサンプル
│   ├── config.toml     # TOML サンプル
│   ├── rules.csv       # CSV サンプル
│   └── global.toml     # グローバル設定テンプレート
doc/                    # ドキュメント
```

---

## 9. `target` と `recursive` の仕様

### 9.1 基本的な考え方

`target` と `recursive` は**独立した2つの軸**で、組み合わせで挙動が決まる。

| 設定 | 役割 |
|------|------|
| **`target`** | **何を**検知するか（ファイル / フォルダ / 両方） |
| **`recursive`** | **どこまで**検知するか（直下のみ / サブディレクトリ含む全階層） |

- `target = "file"` → 監視範囲内の**すべてのファイル**をイベント検知対象にする
- `target = "directory"` → 監視範囲内の**すべてのフォルダ**をイベント検知対象にする（中身の有無は関係ない）
- `target = "both"` → ファイルもフォルダも両方検知する
- `recursive = true` → 監視ルート配下の全階層が監視範囲になる
- `recursive = false` → 監視ルート直下のみが監視範囲になる

**patterns / exclude_patterns によるフィルタは `target` で絞られた後に適用**される。

`include_hidden` による隠しファイルフィルタは `target` フィルタの直後、patterns フィルタの前に適用される。

> フィルタ適用順: **target** → **include_hidden** → **patterns / exclude_patterns**

### 9.2 `target` × `recursive` の組み合わせ

#### (A) `target = "file"` + `recursive = false`（ファイルのみ・直下のみ）

```
C:/data/incoming/          ← 監視ルート
├── report_20260327.csv    ← ★ ファイル → パターンチェック対象
├── readme.txt             ← ★ ファイル → パターンチェック対象
├── sub_folder/            ← ✗ フォルダ → target="file" なので無視
│   ├── report_20260326.csv  ← ✗ recursive=false → 範囲外
│   └── data.json            ← ✗ recursive=false → 範囲外
└── deep/                  ← ✗ フォルダ → 無視
    └── nested/
        └── report.csv     ← ✗ recursive=false → 範囲外
```

#### (B) `target = "file"` + `recursive = true`（ファイルのみ・全階層）

```
C:/data/incoming/          ← 監視ルート
├── report_20260327.csv    ← ★ ファイル → パターンチェック対象
├── readme.txt             ← ★ ファイル → パターンチェック対象
├── sub_folder/            ← ✗ フォルダ → target="file" なので無視
│   ├── report_20260326.csv  ← ★ ファイル＋recursive=true → パターンチェック対象
│   └── data.json            ← ★ ファイル＋recursive=true → パターンチェック対象
└── deep/                  ← ✗ フォルダ → 無視
    └── nested/
        └── report.csv     ← ★ ファイル＋recursive=true → パターンチェック対象
```

#### (C) `target = "directory"` + `recursive = false`（フォルダのみ・直下のみ）

```
C:/data/incoming/          ← 監視ルート
├── report_20260327.csv    ← ✗ ファイル → target="directory" なので無視
├── readme.txt             ← ✗ ファイル → 無視
├── sub_folder/            ← ★ フォルダ → パターンチェック対象（中身は関係ない）
├── empty_folder/          ← ★ 空フォルダでも検知される
└── deep/                  ← ★ フォルダ → パターンチェック対象
    └── nested/            ← ✗ recursive=false → 範囲外
```

#### (D) `target = "directory"` + `recursive = true`（フォルダのみ・全階層）

```
C:/data/incoming/          ← 監視ルート
├── report_20260327.csv    ← ✗ ファイル → 無視
├── sub_folder/            ← ★ フォルダ → パターンチェック対象
│   └── child_folder/      ← ★ recursive=true → パターンチェック対象
└── deep/                  ← ★ フォルダ → パターンチェック対象
    └── nested/            ← ★ recursive=true → パターンチェック対象
```

#### (E) `target = "both"` + `recursive = false`（すべて・直下のみ）

```
C:/data/incoming/          ← 監視ルート
├── report_20260327.csv    ← ★ ファイル → パターンチェック対象
├── readme.txt             ← ★ ファイル → パターンチェック対象
├── sub_folder/            ← ★ フォルダ → パターンチェック対象
│   └── report.csv         ← ✗ recursive=false → 範囲外
└── deep/                  ← ★ フォルダ → パターンチェック対象
```

### 9.3 組み合わせ早見表

| | `recursive = false`（直下のみ） | `recursive = true`（全階層） |
|---|---|---|
| **`target = "file"`** | 直下のファイルのみ検知 | 全階層のファイルを検知 |
| **`target = "directory"`** | 直下のフォルダのみ検知（中身不問） | 全階層のフォルダを検知（中身不問） |
| **`target = "both"`** | 直下のファイル＋フォルダを検知 | 全階層のファイル＋フォルダを検知 |

### 9.4 イベント種別と検知対象の関係（OS技術仕様・参考）

以下はOSレベルで技術的に検知可能かどうかを示す参考表である。**実際にどのイベントを処理するかはユーザーの `events` 設定による。**

| イベント | ファイル | ディレクトリ | 備考 |
|---------|---------|------------|------|
| `create` | ○ | ○ | 新規作成を検知可能 |
| `modify` | ○ | △ | ディレクトリの modify は中身変更時に発生するが挙動はOS依存で不安定 |
| `delete` | ○ | ○ | 削除を検知可能 |
| `rename` | ○ | ○ | リネーム（移動）を検知可能 |

### 9.5 `target="directory"` の設定例

以下はユーザーの命名規則・運用に応じた設定例であり、アプリがフォルダの意味や状態を解釈するわけではない。例えば `patterns=["*_extracted"]` は「`_extracted` で終わるフォルダ名にマッチする」だけであり、フォルダが実際に展開済みかどうかをアプリが判断するわけではない。

| 設定例の意図（ユーザー定義） | 設定 | アクション例 |
|--------|------|------------|
| バッチフォルダ投入を検知したい | `target="directory"`, `patterns=["batch_*"]` | フォルダごとコピー |
| `_extracted` 付きフォルダを検知したい | `target="directory"`, `patterns=["*_extracted"]` | 後処理コマンド実行 |
| 任意のサブフォルダ作成を検知したい | `target="directory"`, `events=["create"]` | ログ記録・通知 |
| 全フォルダを検知したい | `target="directory"`, `patterns=["*"]` | 不要フォルダ削除 |

---

## 10. Windows 固有の考慮事項

### 10.1 パス

- TOML 内のパスは `/` 区切り（`C:/data/incoming`）を推奨。Rust の `std::path::Path` は `/` も `\` も受け付ける
- 長いパス（260文字超）は `\\?\C:\...` プレフィクスで対処。Windows 10 以降はレジストリで Long Path 有効化も可能
- UNC パス（`\\server\share`）対応。SMB で OS 通知が届かない環境は動作保証外

### 10.2 ファイルロック

- Windows では他プロセスが書き込み中のファイルを開けないケースが多い（`SHARING_VIOLATION`）
- コピー/移動前に **排他チェック付きリトライ**（デフォルト3回、1秒間隔）を実装

### 10.3 文字コード

- Windows のファイルシステムは UTF-16 だが、Rust の `OsString` が適切に変換する
- CSV ファイルは BOM 付き UTF-8 を想定（Excel のデフォルト出力が BOM 付き UTF-8）
- ログ出力は UTF-8

### 10.4 プロセス起動

> **確定仕様**: `command` タイプに `shell` 設定を追加（§11.4 参照）。

- `command` タイプ: `shell` 設定に応じて `cmd.exe /C`、`powershell.exe -NoProfile -Command`、`pwsh.exe -NoProfile -Command` のいずれかで実行
- `execute` タイプ: `CreateProcessW` で直接起動（シェルを介さない、安全）
- 環境変数 `%USERPROFILE%` 等の展開はシェル経由時のみ有効

### 10.5 隠しファイル・システムファイル

- `ReadDirectoryChangesW` は隠しファイル（Hidden 属性）やシステムファイル（System 属性）を区別せず、すべてのファイル変更を通知する。API レベルでの除外機能はない
- `include_hidden = false` の場合、アプリ側でイベント受信後に `GetFileAttributesW` でファイル属性を確認し、`FILE_ATTRIBUTE_HIDDEN` が付与されたファイル・フォルダを除外する
- Windows がエクスプローラー操作時に自動生成する `desktop.ini`、`Thumbs.db` 等は隠し属性を持つため、`include_hidden = false` で自動的に除外される
- `include_hidden = true` の場合はフィルタリングせず、OS から通知された全イベントを処理する

#### 属性判定の詳細ルール

| 項目 | 仕様 |
|------|------|
| **判定対象属性** | `FILE_ATTRIBUTE_HIDDEN` のみ。`FILE_ATTRIBUTE_SYSTEM` は判定しない |
| **属性取得失敗時** | delete 等でファイルが既に存在せず `GetFileAttributesW` が失敗した場合、属性判定をスキップし**処理対象とする**（安全側に倒す） |
| **隠しフォルダ配下のファイル** | 個別ファイルの属性のみ判定する。親フォルダの Hidden 属性は遡ってチェックしない（`recursive = true` で隠しフォルダ配下に Hidden 属性を持たない通常ファイルがある場合、そのファイルは処理対象となる） |
| **rename 時** | リネーム後のパスに対して属性判定を行う |

### 10.6 サービス化

- `windows-service` クレートで Windows サービスとして登録可能
- `sc.exe create` / `sc.exe delete` でインストール・アンインストール
- イベントログへの出力は `winlog` クレートで対応可能

---

## 11. 設定ファイル構成（確定仕様）

### 11.1 ファイル分割

設定ファイルは **2ファイル構成** とする。

| ファイル | 用途 | 管理方法 |
|---------|------|---------|
| `global.toml` | グローバル設定（ログ、リトライ等） | 手動編集 |
| `rules.toml` | 監視ルール定義 | CSV→TOML 変換ツールで生成 |

### 11.2 設定方針

- **デフォルト値なし**: すべての設定項目を明示的に記述する必要がある。省略した場合はバリデーションエラーとなる
  - 理由: 動作についてはっきりわかるようにすべて明示的に示す方針
- **相対パスの基準**: 相対パスを指定した場合、CWD（カレントワーキングディレクトリ）を基準とする
- **ホットリロードなし**: 設定変更時はアプリを再起動する（§13.4 参照）

### 11.3 `global.toml` 設定項目

```toml
[global]
log_level = "info"              # trace / debug / info / warn / error
log_file = "./logs/watcher.log" # ログ出力先
log_rotation = "daily"          # daily / never
retry_count = 3                 # アクション失敗時のリトライ回数
retry_interval_ms = 1000        # リトライ間隔（ミリ秒）
dry_run = false                 # ドライランモード
```

| キー | 型 | 必須 | 説明 |
|------|-----|------|------|
| `log_level` | string | ○ | ログレベル: `trace` / `debug` / `info` / `warn` / `error` |
| `log_file` | string | ○ | ログファイルパス |
| `log_rotation` | string | ○ | ログローテーション: `daily`（日次） / `never`（なし） |
| `retry_count` | u32 | ○ | アクション失敗時のリトライ回数 |
| `retry_interval_ms` | u64 | ○ | リトライ間隔（ミリ秒） |
| `dry_run` | bool | ○ | ドライランモード |

> §2 の `debounce_ms` はユーザー設定ではなく内部固定ロジックとして実装する（§14 参照）。
> サイズベースのログローテーションは採用しない。

### 11.4 `rules.toml` 設定項目

§2 のルール定義を基に、以下の変更・追加を行う：

- `shell` フィールドを追加（`command` タイプで必須）
- プレースホルダ名を PowerShell/.NET 準拠に変更（§15 参照）
- 全項目が必須（デフォルト値なし）

```toml
[[rules]]
name = "image-process"
enabled = true

[rules.watch]
path = "C:/data/images"
recursive = false
target = "file"
include_hidden = false                # 隠しファイルを除外
patterns = ["*.png", "*.jpg"]
events = ["create"]

[[rules.actions]]
type = "command"
shell = "powershell"              # cmd / powershell / pwsh
command = "magick convert {FullName} -resize 50% {DirectoryName}/thumb_{Name}"
working_dir = "C:/data/images"
```

#### `rules.toml` の設定項目一覧（§2.2 からの差分・追加）

| セクション | キー | 型 | 必須 | 説明 |
|-----------|------|-----|------|------|
| `rules[].watch` | `include_hidden` | bool | ○ | 隠しファイル・隠しフォルダを検知対象に含めるか。`true`: 含める / `false`: 除外。Windows の `FILE_ATTRIBUTE_HIDDEN` 属性で判定する |
| `rules[].actions[]` | `shell` | string | command 時 ○ | シェル種別: `cmd` / `powershell` / `pwsh` |

#### command タイプの `shell` 設定

| shell 値 | 実行方法 |
|---------|---------|
| `cmd` | `cmd.exe /C <command>` |
| `powershell` | `powershell.exe -NoProfile -Command <command>` |
| `pwsh` | `pwsh.exe -NoProfile -Command <command>` |

### 11.5 バリデーションルール

起動時に以下をチェックし、不正な場合はエラー終了する：

| チェック | レベル |
|---------|-------|
| 全必須項目の存在確認 | エラー |
| `patterns` と `regex` の排他チェック（両方指定は NG） | エラー |
| `patterns` と `regex` の両方省略 | エラー |
| `events` が空配列 | エラー |
| `actions` が空配列 | エラー |
| `action_type` に応じた必須フィールド確認 | エラー |
| `watch_path` の存在確認 | エラー |
| `destination` の存在確認（copy/move 時） | エラー |
| 循環参照チェック（§19.1 参照） | エラー |
| 同一 `name` ルールの watch 設定一致確認 | エラー |

---

## 12. CLI 仕様

### 12.1 watcher（監視アプリ本体）

```
watcher.exe --global <global.toml> --rules <rules.toml> [OPTIONS]
```

| オプション | 短縮 | 必須 | 説明 |
|-----------|------|------|------|
| `--global <path>` | `-g` | ○ | global.toml のパス |
| `--rules <path>` | `-r` | ○ | rules.toml のパス |
| `--dry-run` | | | ドライランモード（global.toml の設定を上書き） |
| `--log-level <level>` | | | ログレベル上書き（global.toml の設定を上書き） |
| `--validate` | | | 設定ファイルのバリデーションのみ実行（監視は開始しない） |
| `--version` | `-V` | | バージョン表示 |
| `--help` | `-h` | | ヘルプ表示 |

### 12.2 csv2toml（変換ツール）

```
csv2toml.exe --input <rules.csv> --output <rules.toml> [OPTIONS]
```

| オプション | 短縮 | 必須 | 説明 |
|-----------|------|------|------|
| `--input <path>` | `-i` | ○ | 入力 CSV ファイルパス |
| `--output <path>` | `-o` | ○ | 出力 TOML ファイルパス |
| `--validate` | | | バリデーションのみ実行 |
| `--dry-run` | | | 変換結果を stdout に出力（ファイル書き出しなし） |
| `--version` | `-V` | | バージョン表示 |
| `--help` | `-h` | | ヘルプ表示 |

### 12.3 終了コード

**watcher**

| コード | 意味 |
|--------|------|
| `0` | 正常終了（グレースフルシャットダウン） |
| `1` | 設定ファイルエラー（パース失敗、バリデーションエラー） |
| `2` | 実行時エラー（監視対象ディレクトリ消失等の致命的エラー） |

**csv2toml**

| コード | 意味 |
|--------|------|
| `0` | 正常終了 |
| `1` | 入力エラー（CSV パース失敗、バリデーションエラー） |

---

## 13. 起動・終了シーケンス

### 13.1 起動シーケンス

```
1. CLI 引数パース
2. global.toml 読み込み・バリデーション
3. rules.toml 読み込み・バリデーション
4. 循環参照チェック（§19.1）
5. ロガー初期化（ファイル出力）
6. glob/regex パターンのコンパイル
7. 監視対象ディレクトリの存在確認
8. 既存ファイルの初回スキャン・処理（§13.3）
9. Watcher 起動（notify）
10. イベントループ開始
```

### 13.2 グレースフルシャットダウン

Ctrl+C（SIGINT）等の終了シグナルを受けた際の動作：

1. **新規イベント受付停止**: Watcher からの新規イベントの処理を停止
2. **実行中アクションの処理**:
   - `copy` / `move`: 完了を待つ
   - `command` / `execute`: fire-and-forget（起動済みプロセスは放置）
3. **ログのフラッシュ**: バッファ内のログを書き出し
4. **正常終了**: 終了コード `0` で終了

### 13.3 再起動時の動作

- **既存ファイルの再処理**: 再起動時、監視対象ディレクトリ内の既存ファイル・フォルダに対してルール評価を実行する
- 過去の処理済みファイルの記録は保持しない（ステートレス）

### 13.4 ホットリロード

**採用しない**。設定変更時はアプリを再起動する。

理由：
- 設定変更は頻繁に行う操作ではない
- ホットリロードの状態管理（Watcher 差し替え、実行中アクションとの整合）が複雑
- 再起動で既存ファイルを再処理する方針（§13.3）と整合する

---

## 14. デバウンス設計

### 14.1 概要

デバウンスは **内部固定ロジック** として実装する（ユーザー設定不要）。

Windows の `ReadDirectoryChangesW` は1つのファイル操作に対して複数のイベント（例: `Create` → `Modify` → `Modify`）を発火することがある。デバウンスにより、これらを1回のアクション実行にまとめる。

### 14.2 仕様

- **デバウンス窓**: コード内定数（500ms）
- **集約キー**: ファイルパス単位
- **集約内容**: デバウンス窓内に発生したイベント種別の **集合** を保持

### 14.3 判定ロジック

```
時間軸: |--- 500ms ---|
OS通知:  Create  Modify  Modify
集約:    {Create, Modify}  ← イベント種別の集合
events設定: ["create"]
判定:    {Create, Modify} ∩ {Create} ≠ ∅ → マッチ → アクション1回実行
```

- デバウンス窓内のイベント種別集合と、ルールの `events` 設定の積集合が空でなければマッチ
- マッチした場合、**1回だけ** アクションを実行する

### 14.4 ルール評価

- **評価順序**: TOML 定義順（上から順に評価）
- **複数ルールマッチ**: 1つのファイルが複数ルールにマッチした場合、全ルールのアクションを実行する
- **Watcher 統合**: 同一 `watch_path` を監視する複数ルールがある場合、OS レベルの Watcher は1つにまとめる
- **rename イベント**: リネーム後のファイル名でパターンマッチを行う

---

## 15. プレースホルダ仕様

### 15.1 プレースホルダ一覧

PowerShell / .NET の `FileInfo` プロパティ名に準拠した命名を採用する。

#### 基本プレースホルダ

検知ファイル `C:/data/sub/report_2026.csv`（watch_path: `C:/data`）の場合：

| プレースホルダ | 値の例 | 説明 |
|---------------|--------|------|
| `{FullName}` | `C:/data/sub/report_2026.csv` | 正規化済み絶対パス |
| `{DirectoryName}` | `C:/data/sub` | 親ディレクトリの絶対パス |
| `{Name}` | `report_2026.csv` | ファイル名（拡張子付き） |
| `{BaseName}` | `report_2026` | ファイル名（拡張子なし） |
| `{Extension}` | `csv` | 拡張子（ドットなし） |

> `{Extension}` は .NET の `Extension` プロパティ（ドット付き `.csv`）とは異なりドットなしを採用する。
> ドット付きで使いたい場合は `.{Extension}` と記述する。拡張子なしファイルの場合、`{Extension}` は空文字列になる。

#### コンテキストプレースホルダ

| プレースホルダ | 値の例 | 説明 |
|---------------|--------|------|
| `{RelativePath}` | `sub/report_2026.csv` | watch_path からの相対パス |
| `{WatchPath}` | `C:/data` | ルールの監視パス |
| `{Destination}` | `C:/backup/sub/report_2026.csv` | 直前の copy/move 先パス（アクションチェーン内のみ有効。直前に copy/move がない場合は空文字列） |

#### 日時プレースホルダ

| プレースホルダ | 値の例 | 説明 |
|---------------|--------|------|
| `{Date}` | `20260328` | アクション実行時の日付（YYYYMMDD） |
| `{Time}` | `143052` | アクション実行時の時刻（HHmmss） |
| `{DateTime}` | `20260328_143052` | 日時（YYYYMMDD_HHmmss） |

### 15.2 エスケープ

リテラルの `{` `}` を出力する場合は `{{` `}}` を使用する。

```
command = "echo {{result}}: {Name}"
→ echo {result}: report_2026.csv
```

### 15.3 未知のプレースホルダ

定義されていないプレースホルダ（例: `{Unknown}`）が含まれる場合、**エラー** とする。起動時のバリデーションで検出する。

### 15.4 アクションチェーン時の更新ルール

アクションチェーン（1ルールに複数アクション）で前のアクションの結果が後続に影響する場合：

| 前アクション | 後続の `{FullName}` 等 | 説明 |
|-------------|----------------------|------|
| `copy` | ソースファイルのパス（変更なし） | ソースは消えないため |
| `move` | **移動後のパス** | ソースファイルは移動済みのため |
| `command` / `execute` | 変更なし | ファイル操作を伴わないため |

`{Destination}` は直前の copy/move アクションの宛先パスが入る。直前に copy/move がない場合は空文字列。

---

## 16. アクション実行仕様

### 16.1 copy（コピー）

- **完了待ち**: コピー完了まで待機してから次のアクションチェーンに進む
- **書き込み方式**: 直接書き込み（一時ファイル経由のアトミック操作は行わない）
- **destination 不在**: エラー（起動時バリデーション）
  - ただし `preserve_structure = true` の場合、**ルートディレクトリが存在すれば OK**（中間ディレクトリは自動作成）
- **フォルダコピー**: `target = "directory"` の場合、フォルダの中身ごとすべて再帰コピーする
- **異ボリューム**: 問題なし（OS がコピーを処理）

### 16.2 move（移動）

- **完了待ち**: 移動完了まで待機
- **書き込み方式**: 直接移動
- **destination 不在**: copy と同様
- **フォルダ移動**: フォルダの中身ごとすべて移動
- **異ボリューム移動**: `rename` API がエラーの場合、内部的に copy → 元ファイル削除にフォールバック

### 16.3 command（コマンド実行）

- **完了待ち**: しない（fire-and-forget）。プロセス起動後に制御を返す
- **shell**: 設定の `shell` フィールドに従い、`cmd.exe` / `powershell.exe` / `pwsh.exe` 経由で実行
- **タイムアウト**: なし
- **stdout / stderr**: ログに記録しない
- **終了コード**: 判定しない
- **working_dir**: 指定された場合はそのディレクトリをカレントディレクトリとして起動

### 16.4 execute（外部プロセス起動）

- **完了待ち**: しない（fire-and-forget）
- **起動方式**: `CreateProcessW` で直接起動（シェルを介さない）
- **タイムアウト**: なし
- **stdout / stderr**: ログに記録しない
- **終了コード**: 判定しない

### 16.5 アクションチェーン

- 1つのルールに複数アクションが定義されている場合、**定義順に順次実行** する
- `copy` / `move` はアクション完了を待ってから次に進む
- `command` / `execute` は起動後すぐに次のアクションに進む（fire-and-forget）
- **エラー時**: チェーン中のアクションがエラーになった場合、**後続アクションを中断** する。エラーをログに記録する

### 16.6 リトライ

- **対象**: ファイル I/O エラーのみ（ファイルロック、一時的な I/O エラー等）
- **設定**: `global.toml` の `retry_count` / `retry_interval_ms` に従う
- **適用範囲**: `copy` / `move` アクションのファイル操作時
- `command` / `execute` はリトライ対象外（fire-and-forget のため）
- 設定バリデーションエラー等の致命的エラーはリトライ対象外

---

## 17. エラー処理

### 17.1 エラー分類

| 分類 | 例 | 動作 |
|------|-----|------|
| **致命的エラー** | 設定ファイルパース失敗、バリデーションエラー、watch_path 消失 | ログ記録 → エラー終了（終了コード `1` or `2`） |
| **回復可能エラー** | ファイルロック、一時的 I/O エラー | リトライ → 失敗時はログ記録してスキップ |
| **アクションエラー** | destination 書き込み失敗、プロセス起動失敗 | ログ記録 → アクションチェーン中断 → 次のイベントへ |

### 17.2 watch_path 消失

監視中に `watch_path` が削除された場合：
- エラーログを記録
- アプリケーションをエラー終了する（終了コード `2`）

---

## 18. ログ仕様

### 18.1 フォーマット

構造化 JSON 形式で出力する。

```json
{"timestamp":"2026-03-28T14:30:52.123+09:00","level":"INFO","event":"file_detected","rule":"csv-backup","event_type":"create","file_path":"C:/data/incoming/report.csv","target":"file"}
{"timestamp":"2026-03-28T14:30:52.456+09:00","level":"INFO","event":"action_started","rule":"csv-backup","action_type":"copy","source":"C:/data/incoming/report.csv","destination":"C:/data/backup/report.csv"}
{"timestamp":"2026-03-28T14:30:52.789+09:00","level":"INFO","event":"action_completed","rule":"csv-backup","action_type":"copy","source":"C:/data/incoming/report.csv","destination":"C:/data/backup/report.csv","duration_ms":333}
{"timestamp":"2026-03-28T14:30:52.789+09:00","level":"ERROR","event":"action_failed","rule":"csv-backup","action_type":"copy","source":"C:/data/incoming/report.csv","error":"ファイルが別プロセスにロックされています","retry":1}
```

#### フィールド一覧

| フィールド | 型 | 常に出力 | 説明 |
|-----------|-----|---------|------|
| `timestamp` | string | ○ | ISO 8601（タイムゾーン付き） |
| `level` | string | ○ | TRACE / DEBUG / INFO / WARN / ERROR |
| `event` | string | ○ | イベント識別子 |
| `rule` | string | | ルール名 |
| `event_type` | string | | create / modify / delete / rename |
| `file_path` | string | | 検知ファイルパス |
| `target` | string | | file / directory |
| `action_type` | string | | copy / move / command / execute |
| `source` | string | | コピー/移動元パス |
| `destination` | string | | コピー/移動先パス |
| `error` | string | | エラーメッセージ |
| `retry` | u32 | | リトライ回数 |
| `duration_ms` | u64 | | 処理時間（ミリ秒） |

#### イベント識別子一覧

| event | 説明 |
|-------|------|
| `app_started` | アプリケーション起動 |
| `app_shutdown` | アプリケーション終了 |
| `file_detected` | ファイル/フォルダ検知 |
| `action_started` | アクション実行開始 |
| `action_completed` | アクション正常完了 |
| `action_failed` | アクション失敗 |
| `action_retry` | アクションリトライ |
| `chain_aborted` | アクションチェーン中断 |
| `validation_error` | 設定バリデーションエラー |
| `watch_path_lost` | 監視ディレクトリ消失 |

### 18.2 ローテーション

| log_rotation 値 | 挙動 | ファイル名 |
|-----------------|------|-----------|
| `daily` | 日次でローテーション | `watcher.YYYY-MM-DD.log` |
| `never` | ローテーションしない | `watcher.log` のまま |

### 18.3 出力先

- **ファイル出力のみ**: `global.toml` の `log_file` に指定されたパスに出力
- コンソール出力は行わない（将来の拡張として別途検討）

---

## 19. 安全性対策

### 19.1 循環参照検知

起動時のバリデーションで以下をチェックする。該当する場合はエラー終了。

1. **destination == watch_path**: コピー/移動先が監視ディレクトリと同一
2. **destination が watch_path の配下** かつ **recursive = true**: コピー先が再帰監視の範囲内
3. **watch_path が destination の配下**: 監視対象がコピー先の内部
4. **ルール間の相互参照**: 全ルールの (watch_path, destination) ペアでグラフを構築し、循環があればエラー

#### 循環参照の例

```
ルールA: watch C:/data/incoming → copy to C:/data/processed
ルールB: watch C:/data/processed → copy to C:/data/incoming
→ A→B→A→B... の無限ループ → エラー
```

### 19.2 シンボリックリンク

**追跡しない（無視）**。`notify` クレートのデフォルト動作に従い、シンボリックリンクの先は監視対象外とする。

### 19.3 ファイル名

以下のファイル名を動作保証する：
- 日本語（マルチバイト文字）
- 半角スペース
- 全角スペース
- 一般的な記号

### 19.4 パストラバーサル防止

コピー・移動先のパスは正規化（`canonicalize`）し、意図しないディレクトリへのアクセスを防止する。

### 19.5 コマンドインジェクション

**サニタイズは実装しない**。

- `command` タイプはシェル経由（cmd.exe / PowerShell）で実行するため、理論上はファイル名に含まれる特殊文字（`&`, `;`, `$()` 等）がシェルコマンドとして解釈されるリスクがある
- ただし、設定ファイル自体に任意コマンドを記述できる時点でセキュリティ境界は設定ファイルの権限管理にある
- **対策**: 設定ファイルの NTFS 権限を適切に管理する。外部ユーザーがファイル名を制御できる環境では `execute` タイプ（シェルを介さない直接起動）の使用を推奨する

---

## 20. CSV→TOML 変換ツール設計（確定仕様）

§8 を基に以下の変更を行う。

### 20.1 変換対象

| 変換元 | 変換先 | 備考 |
|--------|--------|------|
| `rules.csv` | `rules.toml` | 変換ツール対象 |
| `global.toml` | — | 手動編集（変換不要） |

### 20.2 CSV の取り扱い

- **列識別**: ヘッダ名で識別する（列順序に依存しない）
- **空白処理**: 値の前後の空白をトリム処理する。ただし `""` で囲まれた値内の空白は保持する（パスにスペースが含まれるケースに対応）
- **BOM**: BOM 付き UTF-8 を想定（Excel のデフォルト出力）

### 20.3 バリデーション

§8.5 に加え、以下のチェックを行う：

- **同名ルールの設定不一致**: 同じ `name` の複数行で `watch_path` や `patterns` 等の watch 設定が異なる場合はエラー（同名は複数アクションとして統合されるため、watch 設定は一致している必要がある）

### 20.4 CSV フォーマット

§8.2 に `action_shell` 列を追加する：

| 列名 | 型 | 必須 | 説明 |
|------|----|------|------|
| `include_hidden` | bool | ○ | 隠しファイル・隠しフォルダを含めるか: `true` / `false` |
| `action_shell` | string | command 時 ○ | シェル種別: `cmd` / `powershell` / `pwsh` |
