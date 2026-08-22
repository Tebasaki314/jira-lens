use crate::Issue;
use crate::oauth::{JiraResource, SavedSession};
use chrono::{Local, NaiveDateTime, TimeZone};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::time::Duration;

const PAGE_SIZE: u16 = 100;
const MAX_PAGES: usize = 1000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchRequest<'a> {
    jql: &'a str,
    fields: &'a [&'a str],
    fields_by_keys: bool,
    max_results: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_page_token: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    #[serde(default)]
    issues: Vec<ApiIssue>,
    next_page_token: Option<String>,
    #[serde(default)]
    is_last: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectPage {
    #[serde(default)]
    values: Vec<ProjectRef>,
    #[serde(default)]
    start_at: usize,
    #[serde(default)]
    max_results: usize,
    #[serde(default)]
    total: usize,
    #[serde(default)]
    is_last: bool,
}

#[derive(Deserialize)]
struct ProjectRef {
    key: String,
}

#[derive(Deserialize)]
struct ApiIssue {
    key: String,
    fields: ApiFields,
}

#[derive(Default, Deserialize)]
struct ApiFields {
    #[serde(default)]
    summary: String,
    status: Option<NamedValue>,
    issuetype: Option<NamedValue>,
    assignee: Option<NamedValue>,
    duedate: Option<String>,
    timeoriginalestimate: Option<i64>,
    timespent: Option<i64>,
    parent: Option<ParentIssue>,
    description: Option<Value>,
    comment: Option<CommentPage>,
}

#[derive(Deserialize)]
struct NamedValue {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct ParentIssue {
    key: String,
}

#[derive(Deserialize)]
struct CommentPage {
    #[serde(default)]
    comments: Vec<ApiComment>,
}

#[derive(Deserialize)]
struct ApiComment {
    body: Value,
}

pub fn update_issue(
    session: &SavedSession,
    issue_key: &str,
    summary: &str,
    description: &str,
    due: &str,
    estimate_seconds: i64,
) -> Result<(), String> {
    let resource = primary_resource(session)?;
    let client = jira_client()?;
    let endpoint = issue_endpoint(resource, issue_key);
    let due_value = if due.trim().is_empty() {
        Value::Null
    } else {
        Value::String(due.trim().to_owned())
    };
    let body = serde_json::json!({
        "fields": {
            "summary": summary.trim(),
            "description": text_to_adf(description),
            "duedate": due_value,
            "timetracking": {
                "originalEstimate": jira_duration(estimate_seconds)
            }
        }
    });
    send_without_response(
        client
            .put(endpoint)
            .bearer_auth(&session.tokens.access_token)
            .json(&body),
        "Jira課題の更新",
    )
}

fn text_to_adf(text: &str) -> Value {
    let content = text
        .lines()
        .map(|line| {
            if line.is_empty() {
                serde_json::json!({"type": "paragraph"})
            } else {
                serde_json::json!({
                    "type": "paragraph",
                    "content": [{"type": "text", "text": line}]
                })
            }
        })
        .collect::<Vec<_>>();
    serde_json::json!({"type": "doc", "version": 1, "content": content})
}

pub fn add_worklog(
    session: &SavedSession,
    issue_key: &str,
    date: &str,
    time: &str,
    minutes: i32,
) -> Result<(), String> {
    if !(1..=1440).contains(&minutes) {
        return Err("作業時間は1〜1440分で指定してください。".into());
    }
    let naive = NaiveDateTime::parse_from_str(
        &format!("{} {}", date.trim(), time.trim()),
        "%Y-%m-%d %H:%M",
    )
    .map_err(|_| "開始日時は YYYY-MM-DD と HH:MM の形式で入力してください。".to_owned())?;
    let started = Local
        .from_local_datetime(&naive)
        .earliest()
        .ok_or_else(|| "指定された開始日時をローカル時刻へ変換できません。".to_owned())?
        .format("%Y-%m-%dT%H:%M:%S.000%z")
        .to_string();
    let resource = primary_resource(session)?;
    let client = jira_client()?;
    let endpoint = format!("{}/worklog", issue_endpoint(resource, issue_key));
    let body = serde_json::json!({
        "started": started,
        "timeSpentSeconds": i64::from(minutes) * 60
    });
    send_without_response(
        client
            .post(endpoint)
            .bearer_auth(&session.tokens.access_token)
            .json(&body),
        "Jira作業時間の登録",
    )
}

fn primary_resource(session: &SavedSession) -> Result<&JiraResource, String> {
    session
        .resources
        .first()
        .ok_or_else(|| "利用可能なJiraサイトがありません。".to_owned())
}

fn jira_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("Jira HTTPクライアントを作成できません: {error}"))
}

fn issue_endpoint(resource: &JiraResource, issue_key: &str) -> String {
    format!(
        "https://api.atlassian.com/ex/jira/{}/rest/api/3/issue/{}",
        resource.id, issue_key
    )
}

fn send_without_response(
    request: reqwest::blocking::RequestBuilder,
    action: &str,
) -> Result<(), String> {
    let response = request
        .header("Accept", "application/json")
        .send()
        .map_err(|error| format!("{action}に失敗しました: {error}"))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let body = response.text().unwrap_or_default();
    Err(format!("{action}に失敗しました ({status}): {body}"))
}

fn jira_duration(seconds: i64) -> String {
    let minutes = seconds.max(0).saturating_add(59) / 60;
    format!("{minutes}m")
}

pub fn fetch_all_issues(session: &SavedSession) -> Result<(JiraResource, Vec<Issue>), String> {
    let resource = primary_resource(session)?.clone();
    let client = jira_client()?;
    let project_keys = fetch_project_keys(&client, &resource, &session.tokens.access_token)?;
    let fields = [
        "summary",
        "status",
        "issuetype",
        "assignee",
        "duedate",
        "timeoriginalestimate",
        "timespent",
        "parent",
        "description",
        "comment",
    ];
    let mut result = Vec::new();
    for project_key in project_keys {
        result.extend(fetch_project_issues(
            &client,
            &resource,
            &session.tokens.access_token,
            &project_jql(&project_key),
            &fields,
        )?);
    }

    let mut seen = HashSet::new();
    result.retain(|issue| seen.insert(issue.key.clone()));
    Ok((resource, result))
}

fn fetch_project_keys(
    client: &Client,
    resource: &JiraResource,
    access_token: &str,
) -> Result<Vec<String>, String> {
    let endpoint = format!(
        "https://api.atlassian.com/ex/jira/{}/rest/api/3/project/search",
        resource.id
    );
    let mut start_at = 0_usize;
    let mut result = Vec::new();

    for _ in 0..MAX_PAGES {
        let response = client
            .get(&endpoint)
            .bearer_auth(access_token)
            .header("Accept", "application/json")
            .query(&[("startAt", start_at), ("maxResults", PAGE_SIZE as usize)])
            .send()
            .map_err(|error| format!("Jiraプロジェクトの取得に失敗しました: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(format!(
                "Jiraプロジェクトの取得に失敗しました ({status}): {body}"
            ));
        }
        let page = response
            .json::<ProjectPage>()
            .map_err(|error| format!("Jiraプロジェクト情報を解析できません: {error}"))?;
        let item_count = page.values.len();
        result.extend(page.values.into_iter().map(|project| project.key));
        if page.is_last || item_count == 0 || page.start_at.saturating_add(item_count) >= page.total
        {
            return Ok(result);
        }
        start_at = page
            .start_at
            .saturating_add(page.max_results.max(item_count));
    }
    Err("Jiraプロジェクトのページ数が安全上限を超えました。".into())
}

fn fetch_project_issues(
    client: &Client,
    resource: &JiraResource,
    access_token: &str,
    jql: &str,
    fields: &[&str],
) -> Result<Vec<Issue>, String> {
    let endpoint = format!(
        "https://api.atlassian.com/ex/jira/{}/rest/api/3/search/jql",
        resource.id
    );
    let mut next_page_token = None;
    let mut result = Vec::new();

    for _ in 0..MAX_PAGES {
        let request = SearchRequest {
            jql,
            fields,
            fields_by_keys: true,
            max_results: PAGE_SIZE,
            next_page_token: next_page_token.as_deref(),
        };
        let response = client
            .post(&endpoint)
            .bearer_auth(access_token)
            .header("Accept", "application/json")
            .json(&request)
            .send()
            .map_err(|error| format!("Jira課題の取得に失敗しました: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(format!("Jira課題の取得に失敗しました ({status}): {body}"));
        }
        let page = response
            .json::<SearchResponse>()
            .map_err(|error| format!("Jiraレスポンスを解析できません: {error}"))?;
        result.extend(page.issues.into_iter().map(Issue::from));
        if page.is_last || page.next_page_token.is_none() {
            return Ok(result);
        }
        next_page_token = page.next_page_token;
    }
    Err(format!(
        "Jira課題のページ数が安全上限を超えました。対象JQL: {jql}"
    ))
}

fn project_jql(project_key: &str) -> String {
    let escaped = project_key.replace('\\', "\\\\").replace('"', "\\\"");
    format!("project = \"{escaped}\" ORDER BY updated DESC")
}

impl From<ApiIssue> for Issue {
    fn from(issue: ApiIssue) -> Self {
        let fields = issue.fields;
        let comments = fields
            .comment
            .map(|page| {
                page.comments
                    .iter()
                    .map(|comment| adf_to_text(&comment.body))
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        Self {
            key: issue.key,
            summary: fields.summary,
            status: named_value(fields.status),
            issue_type: named_value(fields.issuetype),
            assignee: named_value(fields.assignee),
            due: fields.duedate.unwrap_or_default(),
            estimate: format_duration(fields.timeoriginalestimate),
            spent: format_duration(fields.timespent),
            estimate_seconds: fields.timeoriginalestimate.unwrap_or(0),
            spent_seconds: fields.timespent.unwrap_or(0),
            parent: fields.parent.map(|parent| parent.key).unwrap_or_default(),
            description: fields
                .description
                .as_ref()
                .map(adf_to_text)
                .unwrap_or_default(),
            comments,
        }
    }
}

fn named_value(value: Option<NamedValue>) -> String {
    value
        .and_then(|value| value.display_name.or(value.name))
        .unwrap_or_else(|| "未設定".into())
}

fn format_duration(seconds: Option<i64>) -> String {
    let Some(seconds) = seconds else {
        return "0h".into();
    };
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    match (hours, minutes) {
        (0, 0) => "0h".into(),
        (0, minutes) => format!("{minutes}m"),
        (hours, 0) => format!("{hours}h"),
        (hours, minutes) => format!("{hours}h {minutes}m"),
    }
}

fn adf_to_text(value: &Value) -> String {
    fn visit(value: &Value, output: &mut String) {
        if let Some(text) = value.get("text").and_then(Value::as_str) {
            output.push_str(text);
        }
        if value.get("type").and_then(Value::as_str) == Some("hardBreak") {
            output.push('\n');
        }
        if let Some(content) = value.get("content").and_then(Value::as_array) {
            for child in content {
                visit(child, output);
            }
            if matches!(
                value.get("type").and_then(Value::as_str),
                Some("paragraph" | "heading" | "listItem" | "codeBlock")
            ) {
                output.push('\n');
            }
        }
    }
    let mut output = String::new();
    visit(value, &mut output);
    output.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_adf_to_searchable_text() {
        let adf = serde_json::json!({"type":"doc","version":1,"content":[
            {"type":"paragraph","content":[{"type":"text","text":"最初の行"}]},
            {"type":"paragraph","content":[{"type":"text","text":"次の行"}]}
        ]});
        assert_eq!(adf_to_text(&adf), "最初の行\n次の行");
        assert_eq!(
            adf_to_text(&text_to_adf("最初の行\n次の行")),
            "最初の行\n次の行"
        );
    }

    #[test]
    fn formats_jira_seconds() {
        assert_eq!(format_duration(Some(5 * 3600 + 30 * 60)), "5h 30m");
        assert_eq!(format_duration(None), "0h");
        assert_eq!(jira_duration(90), "2m");
    }

    #[test]
    fn parses_enhanced_search_page_and_issue_fields() {
        let page: SearchResponse = serde_json::from_value(serde_json::json!({
            "issues": [{
                "key": "REAL-1",
                "fields": {
                    "summary": "実課題",
                    "status": {"name": "進行中"},
                    "issuetype": {"name": "タスク"},
                    "assignee": {"displayName": "Hiroshi"},
                    "parent": {"key": "REAL-0"},
                    "timeoriginalestimate": 5400,
                    "timespent": 1800,
                    "description": {"type":"doc","version":1,"content":[]},
                    "comment": {"comments": []}
                }
            }],
            "nextPageToken": "next-token",
            "isLast": false
        }))
        .unwrap();
        assert_eq!(page.next_page_token.as_deref(), Some("next-token"));
        let item = Issue::from(page.issues.into_iter().next().unwrap());
        assert_eq!(item.key, "REAL-1");
        assert_eq!(item.parent, "REAL-0");
        assert_eq!(item.issue_type, "タスク");
        assert_eq!(item.estimate, "1h 30m");
    }

    #[test]
    fn builds_bounded_project_jql() {
        assert_eq!(
            project_jql("TEAM"),
            "project = \"TEAM\" ORDER BY updated DESC"
        );
        assert_eq!(
            project_jql("A\\\"B"),
            "project = \"A\\\\\\\"B\" ORDER BY updated DESC"
        );
    }

    #[test]
    fn parses_project_search_page() {
        let page: ProjectPage = serde_json::from_value(serde_json::json!({
            "startAt": 0,
            "maxResults": 50,
            "total": 2,
            "isLast": true,
            "values": [{"key": "APP"}, {"key": "OPS"}]
        }))
        .unwrap();
        assert_eq!(page.values.len(), 2);
        assert_eq!(page.values[1].key, "OPS");
        assert!(page.is_last);
    }
}
