use crate::oauth::JiraResource;
use crate::{CustomField, Issue};
use directories::ProjectDirs;
use rusqlite::{Connection, params};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

type CachedData = (JiraResource, Vec<Issue>, Vec<CustomField>);

fn database_path() -> Result<PathBuf, String> {
    let dirs = ProjectDirs::from("com", "Tebasaki314", "Jira Lens")
        .ok_or_else(|| "アプリデータフォルダを特定できません。".to_owned())?;
    fs::create_dir_all(dirs.data_local_dir())
        .map_err(|error| format!("キャッシュフォルダを作成できません: {error}"))?;
    Ok(dirs.data_local_dir().join("jira-lens.sqlite3"))
}

fn open() -> Result<Connection, String> {
    let connection = Connection::open(database_path()?)
        .map_err(|error| format!("SQLiteキャッシュを開けません: {error}"))?;
    connection
        .execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS sites (
               id TEXT PRIMARY KEY, name TEXT NOT NULL, url TEXT NOT NULL, last_sync INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS issues (
               site_id TEXT NOT NULL, issue_key TEXT NOT NULL, summary TEXT NOT NULL,
               status TEXT NOT NULL, assignee TEXT NOT NULL, due TEXT NOT NULL,
               estimate TEXT NOT NULL, spent TEXT NOT NULL, parent_key TEXT NOT NULL,
               description TEXT NOT NULL, comments TEXT NOT NULL,
               PRIMARY KEY (site_id, issue_key)
             );
             CREATE INDEX IF NOT EXISTS idx_issues_parent ON issues(site_id, parent_key);",
        )
        .map_err(|error| format!("SQLiteスキーマを準備できません: {error}"))?;
    // 既存ユーザーのキャッシュを破棄せず数値時間列を追加する。
    let _ = connection.execute(
        "ALTER TABLE issues ADD COLUMN estimate_seconds INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE issues ADD COLUMN spent_seconds INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE issues ADD COLUMN issue_type TEXT NOT NULL DEFAULT '未設定'",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE issues ADD COLUMN custom_values TEXT NOT NULL DEFAULT '{}'",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE issues ADD COLUMN remaining TEXT NOT NULL DEFAULT '0h'",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE issues ADD COLUMN remaining_seconds INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE issues ADD COLUMN status_done INTEGER NOT NULL DEFAULT 0",
        [],
    );
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS app_settings (
               setting_key TEXT PRIMARY KEY, setting_value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS custom_fields (
               site_id TEXT NOT NULL, field_id TEXT NOT NULL, name TEXT NOT NULL,
               field_type TEXT NOT NULL, editable INTEGER NOT NULL,
               PRIMARY KEY (site_id, field_id)
             );
             CREATE TABLE IF NOT EXISTS favorite_issues (
               site_id TEXT NOT NULL, issue_key TEXT NOT NULL,
               PRIMARY KEY (site_id, issue_key)
             );",
        )
        .map_err(|error| format!("アプリ設定を準備できません: {error}"))?;
    Ok(connection)
}

pub fn load_column_settings() -> Result<(bool, bool, bool, bool, bool), String> {
    let connection = open()?;
    let value = connection.query_row(
        "SELECT setting_value FROM app_settings WHERE setting_key = 'visible_columns'",
        [],
        |row| row.get::<_, String>(0),
    );
    match value {
        Ok(value) => {
            let flags = value.split(',').collect::<Vec<_>>();
            if flags.len() < 4 {
                return Ok((true, true, true, true, true));
            }
            Ok((
                flags[0] == "1",
                flags[1] == "1",
                flags[2] == "1",
                flags[3] == "1",
                flags.get(4).is_none_or(|value| *value == "1"),
            ))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok((true, true, true, true, true)),
        Err(error) => Err(format!("表示列設定を読めません: {error}")),
    }
}

pub fn save_column_settings(values: (bool, bool, bool, bool, bool)) -> Result<(), String> {
    let connection = open()?;
    let value = [values.0, values.1, values.2, values.3, values.4]
        .into_iter()
        .map(|enabled| if enabled { "1" } else { "0" })
        .collect::<Vec<_>>()
        .join(",");
    connection
        .execute(
            "INSERT INTO app_settings (setting_key, setting_value) VALUES ('visible_columns', ?1)
             ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",
            [value],
        )
        .map_err(|error| format!("表示列設定を保存できません: {error}"))?;
    Ok(())
}

pub fn load_visible_custom_columns() -> Result<Vec<String>, String> {
    load_string_list("visible_custom_columns")
}

pub fn save_visible_custom_columns(ids: &[String]) -> Result<(), String> {
    save_string_list("visible_custom_columns", ids)
}

fn load_string_list(key: &str) -> Result<Vec<String>, String> {
    let connection = open()?;
    match connection.query_row(
        "SELECT setting_value FROM app_settings WHERE setting_key = ?1",
        [key],
        |row| row.get::<_, String>(0),
    ) {
        Ok(value) => {
            serde_json::from_str(&value).map_err(|error| format!("設定を解析できません: {error}"))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Vec::new()),
        Err(error) => Err(format!("設定を読めません: {error}")),
    }
}

fn save_string_list(key: &str, values: &[String]) -> Result<(), String> {
    let connection = open()?;
    let value =
        serde_json::to_string(values).map_err(|error| format!("設定を変換できません: {error}"))?;
    connection
        .execute(
            "INSERT INTO app_settings (setting_key, setting_value) VALUES (?1, ?2)
         ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value",
            params![key, value],
        )
        .map_err(|error| format!("設定を保存できません: {error}"))?;
    Ok(())
}

pub fn load_favorites(site_id: &str) -> Result<std::collections::HashSet<String>, String> {
    let connection = open()?;
    let mut statement = connection
        .prepare("SELECT issue_key FROM favorite_issues WHERE site_id = ?1 ORDER BY issue_key")
        .map_err(|error| format!("お気に入りの読取を準備できません: {error}"))?;
    statement
        .query_map([site_id], |row| row.get(0))
        .map_err(|error| format!("お気に入りを読めません: {error}"))?
        .collect::<Result<_, _>>()
        .map_err(|error| format!("お気に入りを変換できません: {error}"))
}

pub fn set_favorite(site_id: &str, issue_key: &str, favorite: bool) -> Result<(), String> {
    let connection = open()?;
    if favorite {
        connection.execute(
            "INSERT OR IGNORE INTO favorite_issues (site_id, issue_key) VALUES (?1, ?2)",
            params![site_id, issue_key],
        )
    } else {
        connection.execute(
            "DELETE FROM favorite_issues WHERE site_id = ?1 AND issue_key = ?2",
            params![site_id, issue_key],
        )
    }
    .map_err(|error| format!("お気に入りを保存できません: {error}"))?;
    Ok(())
}

pub fn replace_issues(
    resource: &JiraResource,
    issues: &[Issue],
    custom_fields: &[CustomField],
) -> Result<(), String> {
    let mut connection = open()?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("SQLiteトランザクションを開始できません: {error}"))?;
    transaction
        .execute("DELETE FROM issues WHERE site_id = ?1", [&resource.id])
        .map_err(|error| format!("古いキャッシュを削除できません: {error}"))?;
    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO issues (
                   site_id, issue_key, summary, status, assignee, due, estimate, spent,
                   parent_key, description, comments, estimate_seconds, spent_seconds, issue_type, custom_values,
                   remaining, remaining_seconds, status_done
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            )
            .map_err(|error| format!("SQLite保存処理を準備できません: {error}"))?;
        for issue in issues {
            statement
                .execute(params![
                    resource.id,
                    issue.key,
                    issue.summary,
                    issue.status,
                    issue.assignee,
                    issue.due,
                    issue.estimate,
                    issue.spent,
                    issue.parent,
                    issue.description,
                    issue.comments,
                    issue.estimate_seconds,
                    issue.spent_seconds,
                    issue.issue_type,
                    serde_json::to_string(&issue.custom_values).unwrap_or_else(|_| "{}".into()),
                    issue.remaining,
                    issue.remaining_seconds,
                    issue.status_done,
                ])
                .map_err(|error| format!("課題キャッシュを保存できません: {error}"))?;
        }
    }
    transaction
        .execute(
            "DELETE FROM custom_fields WHERE site_id = ?1",
            [&resource.id],
        )
        .map_err(|error| format!("古い項目定義を削除できません: {error}"))?;
    {
        let mut statement = transaction.prepare(
            "INSERT INTO custom_fields (site_id, field_id, name, field_type, editable) VALUES (?1, ?2, ?3, ?4, ?5)"
        ).map_err(|error| format!("項目定義の保存を準備できません: {error}"))?;
        for field in custom_fields {
            statement
                .execute(params![
                    resource.id,
                    field.id,
                    field.name,
                    field.field_type,
                    field.editable as i32
                ])
                .map_err(|error| format!("項目定義を保存できません: {error}"))?;
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    transaction
        .execute(
            "INSERT INTO sites (id, name, url, last_sync) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, url=excluded.url, last_sync=excluded.last_sync",
            params![resource.id, resource.name, resource.url, now],
        )
        .map_err(|error| format!("サイト情報を保存できません: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("SQLiteキャッシュを確定できません: {error}"))
}

pub fn load_latest() -> Result<Option<CachedData>, String> {
    let connection = open()?;
    let site = connection.query_row(
        "SELECT id, name, url FROM sites ORDER BY last_sync DESC LIMIT 1",
        [],
        |row| {
            Ok(JiraResource {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                scopes: Vec::new(),
            })
        },
    );
    let resource = match site {
        Ok(site) => site,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(error) => return Err(format!("キャッシュ済みサイトを読めません: {error}")),
    };
    let mut statement = connection
        .prepare(
            "SELECT issue_key, summary, status, assignee, due, estimate, spent,
                    parent_key, description, comments, estimate_seconds, spent_seconds, issue_type, custom_values,
                    remaining, remaining_seconds, status_done
             FROM issues WHERE site_id = ?1 ORDER BY issue_key",
        )
        .map_err(|error| format!("課題キャッシュの読取を準備できません: {error}"))?;
    let issues = statement
        .query_map([&resource.id], |row| {
            Ok(Issue {
                key: row.get(0)?,
                summary: row.get(1)?,
                status: row.get(2)?,
                status_done: row.get::<_, i32>(16)? != 0 || row.get::<_, String>(2)? == "完了",
                assignee: row.get(3)?,
                due: row.get(4)?,
                estimate: row.get(5)?,
                spent: row.get(6)?,
                parent: row.get(7)?,
                description: row.get(8)?,
                comments: row.get(9)?,
                estimate_seconds: row.get(10)?,
                spent_seconds: row.get(11)?,
                issue_type: row.get(12)?,
                custom_values: serde_json::from_str(&row.get::<_, String>(13)?).unwrap_or_default(),
                remaining: row.get(14)?,
                remaining_seconds: row.get(15)?,
            })
        })
        .map_err(|error| format!("課題キャッシュを読めません: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("課題キャッシュを変換できません: {error}"))?;
    let mut statement = connection.prepare(
        "SELECT field_id, name, field_type, editable FROM custom_fields WHERE site_id = ?1 ORDER BY name"
    ).map_err(|error| format!("項目定義の読取を準備できません: {error}"))?;
    let custom_fields = statement
        .query_map([&resource.id], |row| {
            Ok(CustomField {
                id: row.get(0)?,
                name: row.get(1)?,
                field_type: row.get(2)?,
                editable: row.get::<_, i32>(3)? != 0,
            })
        })
        .map_err(|error| format!("項目定義を読めません: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("項目定義を変換できません: {error}"))?;
    Ok(Some((resource, issues, custom_fields)))
}
