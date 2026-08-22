# Jira Lens

Jira Cloudを、親子ツリー・設定可能な一覧表・時間記録・バーンダウンで見やすくする軽量デスクトップアプリのMVPです。Rust + SlintでmacOS / Windowsを対象にします。

## 現在できること

- 親課題を選び、その配下だけに絞り込み
- キー、タイトル、本文、コメントの横断検索（Enterで実行）
- 表示列（担当者、期限、見積、消費時間）の切り替え
- 課題選択と詳細表示
- 日付カレンダー、見積、開始時刻、作業時間入力のUI
- 親課題単位のバーンダウン表示

現在はモックデータ表示に加えて、Jira OAuth 2.0（3LO）の認可、トークン交換、接続可能サイト取得まで実装済みです。実課題の同期は次の実装段階です。

## 起動

```bash
cargo run
```

## Jira OAuth 2.0設定

1. [Atlassian Developer Console](https://developer.atlassian.com/console/myapps/)でOAuth 2.0（3LO）アプリを作成します。
2. AuthorizationのCallback URLへ `http://127.0.0.1:53682/callback` を登録します。
3. Permissionsで `read:jira-work` と `write:jira-work` を追加します。
4. 開発時だけ、Developer Consoleの値を環境変数へ設定して起動します。

```bash
export JIRA_OAUTH_CLIENT_ID="AtlassianのClient ID"
export JIRA_OAUTH_CLIENT_SECRET="AtlassianのSecret"
export JIRA_OAUTH_REDIRECT_URI="http://127.0.0.1:53682/callback"
cargo run
```

Secretはソースコード、Git、設定ファイルへ保存しないでください。短命のOAuthアクセストークンとサイト情報はメモリだけに保持し、ローテーション方式のリフレッシュトークンだけをmacOS KeychainまたはWindows Credential Managerへ保存します。Windows Credential Managerの1項目あたりのサイズ制限に対応するため、長いトークンは複数の資格情報へ分割します。

公開配布版では利用者ごとに3LOアプリを作らせず、一つの登録済みOAuthアプリとHTTPSコールバックサービスを使用します。デスクトップバイナリへClient Secretを埋め込まないための認証ブローカーは今後の配布工程で追加します。

OAuth API呼び出しは `https://api.atlassian.com/ex/jira/{cloudId}/...` を使用します。

主要API:

- 課題検索: `POST /rest/api/3/search/jql`
- 課題更新: `PUT /rest/api/3/issue/{issueIdOrKey}`
- コメント: `GET /rest/api/3/issue/{issueIdOrKey}/comment`
- 作業時間: `POST /rest/api/3/issue/{issueIdOrKey}/worklog`

## ロードマップ

1. ✅ Jira OAuth 2.0認証、OS資格情報ストアへのトークン保存
2. Jira同期とSQLiteキャッシュ
3. 実データによるツリー、検索、編集、作業時間登録
4. 履歴スナップショットによる正確なバーンダウン
5. 認証ブローカー、macOS署名・公証、Windows署名、更新配布
