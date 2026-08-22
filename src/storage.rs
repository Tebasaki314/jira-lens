use crate::Issue;
use crate::oauth::JiraResource;
use directories::ProjectDirs;
use rusqlite::{Connection, params};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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
    Ok(connection)
}

pub fn replace_issues(resource: &JiraResource, issues: &[Issue]) -> Result<(), String> {
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
                   parent_key, description, comments
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                ])
                .map_err(|error| format!("課題キャッシュを保存できません: {error}"))?;
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

pub fn load_latest() -> Result<Option<(String, Vec<Issue>)>, String> {
    let connection = open()?;
    let site = connection.query_row(
        "SELECT id, name FROM sites ORDER BY last_sync DESC LIMIT 1",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    );
    let (site_id, site_name) = match site {
        Ok(site) => site,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(error) => return Err(format!("キャッシュ済みサイトを読めません: {error}")),
    };
    let mut statement = connection
        .prepare(
            "SELECT issue_key, summary, status, assignee, due, estimate, spent,
                    parent_key, description, comments
             FROM issues WHERE site_id = ?1 ORDER BY issue_key",
        )
        .map_err(|error| format!("課題キャッシュの読取を準備できません: {error}"))?;
    let issues = statement
        .query_map([site_id], |row| {
            Ok(Issue {
                key: row.get(0)?,
                summary: row.get(1)?,
                status: row.get(2)?,
                assignee: row.get(3)?,
                due: row.get(4)?,
                estimate: row.get(5)?,
                spent: row.get(6)?,
                parent: row.get(7)?,
                description: row.get(8)?,
                comments: row.get(9)?,
            })
        })
        .map_err(|error| format!("課題キャッシュを読めません: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("課題キャッシュを変換できません: {error}"))?;
    Ok(Some((site_name, issues)))
}
