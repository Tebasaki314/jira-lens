# Jira Lens

Jira Cloudを、親子ツリー・設定可能な一覧表・時間記録・バーンダウンで見やすくする軽量デスクトップアプリのMVPです。Rust + SlintでmacOS / Windowsを対象にします。

## 現在できること

- 親課題を選び、その配下だけに絞り込み
- キー、タイトル、本文、コメントの横断検索（Enterで実行）
- 表示列（担当者、期限、見積、消費時間）の切り替え
- 表示列設定の端末内保存
- 課題のタイトル・本文・期限・初期見積もり更新
- 実カレンダーによる日付選択、開始日時と作業時間のworklog登録
- 同期・更新時の履歴スナップショットによる親課題単位のバーンダウン

Jira OAuth 2.0（3LO）で認証し、JQL拡張検索から実課題を同期できます。同期した課題はSQLiteへキャッシュされ、次回起動時は未接続でもツリー・一覧・本文・コメント検索を利用できます。初回同期前だけデモデータを表示します。

## 起動

```bash
cargo run
```

初期見積もりはJiraと同じ `1d 2h 30m` 形式で入力します。このアプリでは1日を8時間として扱います。Jiraへの書き込みはOAuth接続済みの間だけ有効です。キャッシュから起動した場合は、先に「再同期」を実行してください。

## Windows

PowerShellでOAuth設定を行い、同じターミナルから起動します。

```powershell
$env:JIRA_OAUTH_CLIENT_ID="AtlassianのClient ID"
$env:JIRA_OAUTH_CLIENT_SECRET="AtlassianのSecret"
$env:JIRA_OAUTH_REDIRECT_URI="http://127.0.0.1:53682/callback"
cargo run --release
```

GitHub ActionsはmainへのpushごとにmacOS / Windowsの未署名リリースバイナリを作成します。GitHubのActions画面からArtifactsをダウンロードできます。OS署名・macOS公証は証明書を用意した配布時に追加します。

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
2. ✅ Jira同期とSQLiteキャッシュ、実データによるツリー・一覧・検索
3. ✅ 課題編集、日付更新、作業時間登録
4. ✅ 履歴スナップショットによるバーンダウン
5. 🟡 macOS / Windows自動ビルド（完了）、認証ブローカー・署名・公証（配布資格情報の準備後）

## 配布について

現在のバイナリは個人利用・開発版です。追加料金なしで利用できますが、OAuth Client Secretをバイナリへ埋め込まない設計のため、起動環境でOAuth設定が必要です。不特定多数へ安全に配布する段階ではHTTPS認証ブローカーと署名証明書が必要です。これらはアプリ本体とは独立した配布工程として扱います。
