use crate::Issue;
use crate::oauth::{JiraResource, SavedSession};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
struct ApiIssue {
    key: String,
    fields: ApiFields,
}

#[derive(Default, Deserialize)]
struct ApiFields {
    #[serde(default)]
    summary: String,
    status: Option<NamedValue>,
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

pub fn fetch_all_issues(session: &SavedSession) -> Result<(JiraResource, Vec<Issue>), String> {
    let resource = session
        .resources
        .first()
        .cloned()
        .ok_or_else(|| "利用可能なJiraサイトがありません。".to_owned())?;
    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("Jira HTTPクライアントを作成できません: {error}"))?;
    let endpoint = format!(
        "https://api.atlassian.com/ex/jira/{}/rest/api/3/search/jql",
        resource.id
    );
    let fields = [
        "summary",
        "status",
        "assignee",
        "duedate",
        "timeoriginalestimate",
        "timespent",
        "parent",
        "description",
        "comment",
    ];
    let mut next_page_token = None;
    let mut result = Vec::new();

    for _ in 0..MAX_PAGES {
        let request = SearchRequest {
            jql: "ORDER BY updated DESC",
            fields: &fields,
            fields_by_keys: true,
            max_results: PAGE_SIZE,
            next_page_token: next_page_token.as_deref(),
        };
        let response = client
            .post(&endpoint)
            .bearer_auth(&session.tokens.access_token)
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
            return Ok((resource, result));
        }
        next_page_token = page.next_page_token;
    }
    Err("Jira課題のページ数が安全上限を超えました。".into())
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
            assignee: named_value(fields.assignee),
            due: fields.duedate.unwrap_or_default(),
            estimate: format_duration(fields.timeoriginalestimate),
            spent: format_duration(fields.timespent),
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
    }

    #[test]
    fn formats_jira_seconds() {
        assert_eq!(format_duration(Some(5 * 3600 + 30 * 60)), "5h 30m");
        assert_eq!(format_duration(None), "0h");
    }

    #[test]
    fn parses_enhanced_search_page_and_issue_fields() {
        let page: SearchResponse = serde_json::from_value(serde_json::json!({
            "issues": [{
                "key": "REAL-1",
                "fields": {
                    "summary": "実課題",
                    "status": {"name": "進行中"},
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
        assert_eq!(item.estimate, "1h 30m");
    }
}
