# Git ブランチ戦略・プッシュ戦略・工数見積もり

| 項目 | 内容 |
|------|------|
| 文書版数 | 1.0 |
| 作成日 | 2026-03-29 |
| 前提 | 実装計画書 v1.0 / 実装者: Rust 初心者（1 名） |

---

## 目次

1. [ブランチ戦略の選定](#1-ブランチ戦略の選定)
2. [ブランチ命名規則](#2-ブランチ命名規則)
3. [ブランチ運用フロー](#3-ブランチ運用フロー)
4. [プッシュ戦略](#4-プッシュ戦略)
5. [フェーズ別工数見積もり](#5-フェーズ別工数見積もり)
6. [全体スケジュール](#6-全体スケジュール)
7. [リスクと対策](#7-リスクと対策)

---

## 1. ブランチ戦略の選定

### 1.1 候補の比較

```mermaid
graph LR
    A["Git Flow"] -->|複雑| X["❌ 不採用"]
    B["GitHub Flow"] -->|シンプル| Y["✅ ベース採用"]
    C["Trunk-Based"] -->|CI必須| Z["❌ 不採用"]
```

| 戦略 | メリット | デメリット | 本プロジェクトへの適性 |
|------|---------|-----------|---------------------|
| **Git Flow** | リリース管理が厳密 | develop/release/hotfix 等ブランチが多く複雑 | ❌ 1 人開発には過剰 |
| **GitHub Flow** | シンプル、main + feature branch のみ | リリース管理は別途必要 | ✅ **最適** |
| **Trunk-Based** | マージ地獄なし | CI/CD 必須、小さいコミット強制 | ❌ 初心者にはハードル高 |

### 1.2 採用戦略: **Phase Branch Flow**（GitHub Flow ベース）

本プロジェクトは **実装計画のフェーズ単位でブランチを切る** 戦略を採用する。

**理由:**

- 1 人開発のため、レビュー・承認フローは不要
- フェーズ = 機能単位のため、ブランチの粒度が明確
- 各フェーズ完了時に `main` へマージすることで、常に動く状態を `main` に保てる
- Rust 初心者のため「壊したら戻せる」安心感が重要

```mermaid
gitGraph
    commit id: "init"
    branch phase/0-project-setup
    commit id: "workspace構造"
    commit id: "空モジュール作成"
    checkout main
    merge phase/0-project-setup id: "Phase 0 完了" tag: "v0.0"
    branch phase/1-hello-world
    commit id: "main.rs実装"
    commit id: "mod宣言"
    checkout main
    merge phase/1-hello-world id: "Phase 1 完了" tag: "v0.1"
    branch phase/2-error-types
    commit id: "thiserror追加"
    commit id: "AppError定義"
    commit id: "テスト追加"
    checkout main
    merge phase/2-error-types id: "Phase 2 完了" tag: "v0.2"
    branch phase/3-config
    commit id: "serde構造体"
    commit id: "TOMLパース"
    commit id: "テスト"
    checkout main
    merge phase/3-config id: "Phase 3 完了" tag: "v0.3"
```

---

## 2. ブランチ命名規則

### 2.1 基本ルール

```
phase/<番号>-<英語の短い説明>
```

### 2.2 全ブランチ一覧

| Phase | ブランチ名 | 元ブランチ |
|-------|-----------|-----------|
| 0 | `phase/0-project-setup` | `main` |
| 1 | `phase/1-hello-world` | `main` |
| 2 | `phase/2-error-types` | `main` |
| 3 | `phase/3-config-loading` | `main` |
| 4 | `phase/4-cli` | `main` |
| 5 | `phase/5-validation` | `main` |
| 6 | `phase/6-placeholder` | `main` |
| 7 | `phase/7-file-watcher` | `main` |
| 8 | `phase/8-router-debounce` | `main` |
| 9 | `phase/9-action-copy` | `main` |
| 10 | `phase/10-action-move` | `main` |
| 11 | `phase/11-action-cmd-exec` | `main` |
| 12 | `phase/12-integration` | `main` |
| 13 | `phase/13-csv2toml` | `main` |
| 14 | `phase/14-final-testing` | `main` |

### 2.3 補助ブランチ（必要に応じて）

| 用途 | 命名 | 例 |
|------|------|----|
| バグ修正 | `fix/<対象>-<説明>` | `fix/config-parse-error` |
| ドキュメント | `doc/<説明>` | `doc/update-readme` |
| 手戻り・再実装 | `phase/<番号>-<説明>-v2` | `phase/5-validation-v2` |

---

## 3. ブランチ運用フロー

### 3.1 フェーズ単位のワークフロー

```mermaid
flowchart TD
    A["main から最新を pull"] --> B["phase/N-xxx ブランチを作成"]
    B --> C["実装・ビルド確認"]
    C --> D{"cargo check\n通る？"}
    D -->|No| C
    D -->|Yes| E["テスト実装"]
    E --> F{"cargo test\n通る？"}
    F -->|No| C
    F -->|Yes| G["コミット & プッシュ"]
    G --> H{"フェーズ完了\n条件を満たす？"}
    H -->|No| C
    H -->|Yes| I["main にマージ"]
    I --> J["タグ付け v0.N"]
    J --> K["phase ブランチ削除"]

    style A fill:#e1f5fe
    style I fill:#c8e6c9
    style K fill:#fff3e0
```

### 3.2 コミットメッセージ規約

```
<type>(<scope>): <description>

type:
  feat     - 新機能
  fix      - バグ修正
  test     - テスト追加・修正
  refactor - リファクタリング
  docs     - ドキュメント
  chore    - ビルド設定・依存追加等

scope:
  config, watcher, router, actions, placeholder, error, cli, csv2toml

例:
  feat(config): global.toml のデシリアライズ実装
  test(config): 必須項目欠落時のエラーテスト追加
  chore: serde, toml クレートを追加
```

### 3.3 マージ方針

```mermaid
flowchart LR
    subgraph "推奨: Squash Merge"
        A1["phase ブランチの\n複数コミット"] --> B1["1 コミットに\nまとめて main へ"]
    end
    subgraph "代替: 通常 Merge"
        A2["phase ブランチの\n複数コミット"] --> B2["マージコミット\nで main へ"]
    end

    style B1 fill:#c8e6c9
    style B2 fill:#fff9c4
```

| 方式 | 使い分け |
|------|---------|
| **Squash Merge**（推奨） | フェーズ完了時。main の履歴がフェーズ単位でクリーンになる |
| **通常 Merge** | 途中経過を残したいフェーズ（Phase 12 等の大きなフェーズ） |

---

## 4. プッシュ戦略

### 4.1 プッシュのタイミング

```mermaid
flowchart TD
    subgraph "日次プッシュ（推奨）"
        D1["作業開始"] --> D2["実装"]
        D2 --> D3["ビルド確認"]
        D3 --> D4["コミット"]
        D4 --> D5{"作業終了？"}
        D5 -->|No| D2
        D5 -->|Yes| D6["git push origin phase/N-xxx"]
    end

    subgraph "マイルストーンプッシュ"
        M1["サブタスク完了"] --> M2["コミット + プッシュ"]
        M3["テスト全件パス"] --> M4["コミット + プッシュ"]
        M5["フェーズ完了"] --> M6["main マージ + プッシュ + タグ"]
    end

    style D6 fill:#c8e6c9
    style M6 fill:#c8e6c9
```

### 4.2 プッシュルール

| ルール | 内容 |
|--------|------|
| **日次バックアップ** | 作業終了時に必ず push（未完成でも OK） |
| **ビルド確認後** | `cargo check` が通らないコードは push しない |
| **main へのプッシュ** | マージ完了 + タグ付け後にのみ push |
| **force push** | phase ブランチでは許可（自分専用）/ main では **禁止** |

### 4.3 タグ戦略

```mermaid
timeline
    title リリースタグのタイムライン
    section 基盤構築
        v0.0 : Phase 0 プロジェクト構築
        v0.1 : Phase 1 Hello World
    section コア機能
        v0.2 : Phase 2 エラー型
        v0.3 : Phase 3 設定読み込み
        v0.4 : Phase 4 CLI
        v0.5 : Phase 5 バリデーション
        v0.6 : Phase 6 プレースホルダ
    section 監視エンジン
        v0.7 : Phase 7 ファイル監視
        v0.8 : Phase 8 ルーター
    section アクション
        v0.9  : Phase 9 copy
        v0.10 : Phase 10 move
        v0.11 : Phase 11 command/execute
    section 統合
        v0.12 : Phase 12 統合・仕上げ
        v0.13 : Phase 13 csv2toml
        v1.0  : Phase 14 総合テスト完了
```

---

## 5. フェーズ別工数見積もり

### 5.1 見積もり前提

| 項目 | 前提条件 |
|------|---------|
| 実装者 | 1 名（Rust 初心者） |
| 1 日の作業時間 | 4〜6 時間（学習時間含む） |
| 見積もり単位 | 人日（学習 + 実装 + テスト + デバッグの合計） |
| バッファ | 各フェーズに不確実性に応じた係数を適用 |

### 5.2 フェーズ別内訳

```mermaid
gantt
    title cat-watcher / csv2toml 実装スケジュール
    dateFormat  YYYY-MM-DD
    axisFormat  %m/%d

    section 基盤構築
    Phase 0 プロジェクト構築        :p0, 2026-03-31, 1d
    Phase 1 Hello World            :p1, after p0, 1d

    section コア機能
    Phase 2 エラー型定義            :p2, after p1, 2d
    Phase 3 設定読み込み            :p3, after p2, 4d
    Phase 4 CLI                    :p4, after p3, 2d
    Phase 5 バリデーション          :p5, after p4, 5d
    Phase 6 プレースホルダ          :p6, after p3, 4d

    section 監視エンジン
    Phase 7 ファイル監視            :p7, after p5, 5d
    Phase 8 ルーター＋デバウンス    :p8, after p7, 5d

    section アクション
    Phase 9 アクション copy         :p9, after p8, 4d
    Phase 10 アクション move        :p10, after p9, 3d
    Phase 11 command / execute      :p11, after p10, 3d

    section 統合・仕上げ
    Phase 12 統合・仕上げ           :p12, after p11, 7d
    Phase 13 csv2toml              :p13, after p12, 4d
    Phase 14 総合テスト            :p14, after p13, 4d
```

### 5.3 詳細見積もりテーブル

| Phase | 名前 | 学習 | 実装 | テスト | バッファ | **合計(日)** | 難易度 | リスク |
|-------|------|------|------|--------|---------|------------|--------|--------|
| 0 | プロジェクト構築 | 0.5 | 0.5 | — | — | **1** | ★☆☆☆☆ | 低 |
| 1 | Hello World | 0.5 | 0.5 | — | — | **1** | ★☆☆☆☆ | 低 |
| 2 | エラー型定義 | 1 | 0.5 | 0.5 | — | **2** | ★★☆☆☆ | 低 |
| 3 | 設定読み込み | 1 | 1.5 | 1 | 0.5 | **4** | ★★★☆☆ | 中 |
| 4 | CLI | 0.5 | 1 | 0.5 | — | **2** | ★★☆☆☆ | 低 |
| 5 | バリデーション | 1 | 2 | 1.5 | 0.5 | **5** | ★★★★☆ | 高 |
| 6 | プレースホルダ | 0.5 | 2 | 1 | 0.5 | **4** | ★★★☆☆ | 中 |
| 7 | ファイル監視 | 2 | 1.5 | 0.5 | 1 | **5** | ★★★★☆ | 高 |
| 8 | ルーター＋デバウンス | 1 | 2 | 1.5 | 0.5 | **5** | ★★★★☆ | 高 |
| 9 | コピー | 0.5 | 2 | 1 | 0.5 | **4** | ★★★☆☆ | 中 |
| 10 | 移動 | 0.5 | 1.5 | 0.5 | 0.5 | **3** | ★★★☆☆ | 中 |
| 11 | command / execute | 0.5 | 1.5 | 0.5 | 0.5 | **3** | ★★★☆☆ | 中 |
| 12 | 統合・仕上げ | 1 | 3 | 1 | 2 | **7** | ★★★★★ | 最高 |
| 13 | csv2toml | 1 | 2 | 0.5 | 0.5 | **4** | ★★★☆☆ | 中 |
| 14 | 総合テスト | — | 1 | 2 | 1 | **4** | ★★★☆☆ | 中 |
| | | | | | **合計** | **54 日** | | |

### 5.4 工数サマリー

```mermaid
pie title 工数配分（全54日）
    "基盤構築 (Phase 0-1)" : 2
    "コア機能 (Phase 2-6)" : 17
    "監視エンジン (Phase 7-8)" : 10
    "アクション (Phase 9-11)" : 10
    "統合・仕上げ (Phase 12)" : 7
    "csv2toml + テスト (Phase 13-14)" : 8
```

---

## 6. 全体スケジュール

### 6.1 想定スケジュール（3 シナリオ）

| シナリオ | 想定 | 期間 | 完了予定 |
|---------|------|------|---------|
| **楽観** | 学習スムーズ・ハマりなし | 約 40 日 | 2026-05-15 頃 |
| **標準** | 適度にハマる（見積もり通り） | 約 54 日 | 2026-06-01 頃 |
| **悲観** | 所有権/async で大きくハマる | 約 70 日 | 2026-06-20 頃 |

### 6.2 マイルストーン

```mermaid
graph LR
    MS1["🏁 MS1<br/>基盤完成<br/>Phase 0-1<br/>~2日目"]
    MS2["🏁 MS2<br/>設定完成<br/>Phase 2-5<br/>~15日目"]
    MS3["🏁 MS3<br/>監視動作<br/>Phase 7-8<br/>~25日目"]
    MS4["🏁 MS4<br/>アクション完成<br/>Phase 9-11<br/>~35日目"]
    MS5["🏁 MS5<br/>cat-watcher完成<br/>Phase 12<br/>~42日目"]
    MS6["🏗️ MS6<br/>v1.0 リリース<br/>Phase 13-14<br/>~54日目"]

    MS1 --> MS2 --> MS3 --> MS4 --> MS5 --> MS6

    style MS1 fill:#e3f2fd
    style MS2 fill:#e8f5e9
    style MS3 fill:#fff3e0
    style MS4 fill:#fce4ec
    style MS5 fill:#f3e5f5
    style MS6 fill:#e0f7fa
```

### 6.3 クリティカルパスの可視化

```mermaid
flowchart LR
    P0["Phase 0<br/>1日"] --> P1["Phase 1<br/>1日"]
    P1 --> P2["Phase 2<br/>2日"]
    P2 --> P3["Phase 3<br/>4日"]
    P3 --> P4["Phase 4<br/>2日"]
    P4 --> P5["Phase 5<br/>5日"]
    P3 --> P6["Phase 6<br/>4日"]
    P5 --> P7["Phase 7<br/>5日"]
    P7 --> P8["Phase 8<br/>5日"]
    P8 --> P9["Phase 9<br/>4日"]
    P9 --> P10["Phase 10<br/>3日"]
    P10 --> P11["Phase 11<br/>3日"]
    P11 --> P12["Phase 12<br/>7日"]
    P12 --> P13["Phase 13<br/>4日"]
    P13 --> P14["Phase 14<br/>4日"]
    P6 -.->|"プレースホルダ検証統合"| P5

    style P0 fill:#ffcdd2
    style P1 fill:#ffcdd2
    style P2 fill:#ffcdd2
    style P3 fill:#ffcdd2
    style P4 fill:#ffcdd2
    style P5 fill:#ffcdd2
    style P7 fill:#ffcdd2
    style P8 fill:#ffcdd2
    style P9 fill:#ffcdd2
    style P12 fill:#ffcdd2

    linkStyle 0 stroke:#f44336,stroke-width:3px
    linkStyle 1 stroke:#f44336,stroke-width:3px
    linkStyle 2 stroke:#f44336,stroke-width:3px
    linkStyle 3 stroke:#f44336,stroke-width:3px
    linkStyle 5 stroke:#f44336,stroke-width:3px
    linkStyle 6 stroke:#f44336,stroke-width:3px
    linkStyle 7 stroke:#f44336,stroke-width:3px
    linkStyle 8 stroke:#f44336,stroke-width:3px
```

> 赤いノード＝クリティカルパス上のフェーズ。ここが遅れると全体に影響する。

---

## 7. リスクと対策

### 7.1 技術リスク

```mermaid
quadrantChart
    title 技術リスクマトリクス
    x-axis "影響度 低" --> "影響度 高"
    y-axis "発生確率 低" --> "発生確率 高"
    quadrant-1 "重点対策"
    quadrant-2 "監視"
    quadrant-3 "許容"
    quadrant-4 "軽減策実施"
    "所有権と借用で詰まる": [0.75, 0.85]
    "async await理解不足": [0.70, 0.65]
    "notify動作不安定": [0.60, 0.40]
    "Windows API呼び出し": [0.45, 0.50]
    "循環参照検出の実装": [0.55, 0.35]
    "デバウンス設計ミス": [0.65, 0.30]
```

### 7.2 リスク対策表

| リスク | 影響フェーズ | 対策 |
|--------|------------|------|
| 所有権/借用で長期停滞 | Phase 3〜全般 | 公式 Book の該当章を先読み。Clone で逃げてから後でリファクタ |
| async/await の理解不足 | Phase 7〜 | tokio チュートリアルを Phase 6 の間に消化 |
| notify がイベントを取りこぼす | Phase 7-8 | 手動テストスクリプトで早期検証。Rescan 対応を Phase 12 で実装 |
| Phase 12 の肥大化 | Phase 12 | サブタスク（12-A〜12-F）ごとにコミット。必要なら分割ブランチ化 |
| 見積もり超過 | 全般 | 週次で進捗確認。2 倍以上遅れたら計画見直し |

### 7.3 Phase 12 の分割ブランチ（必要に応じて）

Phase 12 は工数が最大のため、膨らんだ場合はサブブランチに分割する。

```mermaid
gitGraph
    commit id: "Phase 11 完了"
    branch phase/12-integration
    commit id: "12開始"
    branch phase/12a-logging
    commit id: "tracing設定"
    commit id: "JSON出力"
    checkout phase/12-integration
    merge phase/12a-logging id: "12-A完了"
    branch phase/12b-hidden-filter
    commit id: "GetFileAttributesW"
    checkout phase/12-integration
    merge phase/12b-hidden-filter id: "12-B完了"
    branch phase/12c-initial-scan
    commit id: "初回スキャン"
    checkout phase/12-integration
    merge phase/12c-initial-scan id: "12-C完了"
    branch phase/12d-shutdown
    commit id: "Ctrl+C処理"
    checkout phase/12-integration
    merge phase/12d-shutdown id: "12-D完了"
    checkout main
    merge phase/12-integration id: "Phase 12 完了" tag: "v0.12"
```

---

## 付録: Git コマンドチートシート

```powershell
# --- フェーズ開始 ---
git checkout main
git pull origin main
git checkout -b phase/N-xxx

# --- 日次作業 ---
git add -A
git commit -m "feat(scope): 説明"
git push origin phase/N-xxx

# --- フェーズ完了 ---
git checkout main
git merge --squash phase/N-xxx
git commit -m "feat: Phase N - 説明"
git tag v0.N
git push origin main --tags
git branch -d phase/N-xxx
git push origin --delete phase/N-xxx
```
