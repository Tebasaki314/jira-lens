# Jira Lens

Jira Cloudを、親子ツリー・設定可能な一覧表・時間記録・バーンダウンで見やすくする軽量デスクトップアプリのMVPです。Rust + SlintでmacOS / Windowsを対象にします。

## 現在できること

- 親課題を選び、その配下だけに絞り込み
- キー、タイトル、本文、コメントの横断検索（Enterで実行）
- 表示列（担当者、期限、見積、消費時間）の切り替え
- 課題選択と詳細表示
- 日付カレンダー、見積、開始時刻、作業時間入力のUI
- 親課題単位のバーンダウン表示

現在は安全に試せるモックデータ版です。Jira REST API接続は次の実装段階です。

## 起動

```bash
cargo run
```

## Jira接続の方針

個人利用MVPでは、サイトURL・メールアドレス・APIトークンをOSの資格情報ストアに保存し、Jira Cloud REST API v3へ直接接続します。配布する製品ではOAuth 2.0 (3LO)へ移行します。

主要API:

- 課題検索: `POST /rest/api/3/search/jql`
- 課題更新: `PUT /rest/api/3/issue/{issueIdOrKey}`
- コメント: `GET /rest/api/3/issue/{issueIdOrKey}/comment`
- 作業時間: `POST /rest/api/3/issue/{issueIdOrKey}/worklog`

## ロードマップ

1. Jira認証と同期、SQLiteキャッシュ
2. 実データによるツリー、検索、編集、作業時間登録
3. 履歴スナップショットによる正確なバーンダウン
4. macOS署名・公証、Windows署名、更新配布

