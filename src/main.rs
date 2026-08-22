mod jira;
mod oauth;
mod storage;

use slint::{Model, ModelRc, SharedString, VecModel};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

slint::include_modules!();

#[derive(Clone, Debug)]
struct Issue {
    key: String,
    summary: String,
    status: String,
    assignee: String,
    due: String,
    estimate: String,
    spent: String,
    parent: String,
    description: String,
    comments: String,
}

fn demo_issues() -> Vec<Issue> {
    vec![
        issue(
            "APP-100",
            "デスクトップ版 v1",
            "進行中",
            "Hiroshi",
            "2026-09-12",
            "40h",
            "12h",
            "",
            "Jiraを軽快に閲覧・更新するデスクトップアプリ",
            "MVPの対象範囲を確定",
        ),
        issue(
            "APP-101",
            "親子ツリー表示",
            "完了",
            "Hiroshi",
            "2026-08-24",
            "8h",
            "7h 30m",
            "APP-100",
            "Epic、親タスク、サブタスクを一つのツリーで表示",
            "仮想リストを使用する",
        ),
        issue(
            "APP-102",
            "課題一覧テーブル",
            "進行中",
            "Mika",
            "2026-08-28",
            "12h",
            "4h",
            "APP-100",
            "Excelのように見通しのよい一覧を作る",
            "列の表示切替を追加",
        ),
        issue(
            "APP-105",
            "日付セルの編集",
            "未着手",
            "Mika",
            "2026-08-27",
            "3h",
            "0h",
            "APP-102",
            "カレンダーから期限を入力できるようにする",
            "キーボード操作も後で対応",
        ),
        issue(
            "APP-103",
            "時間記録フォーム",
            "レビュー",
            "Sora",
            "2026-08-30",
            "6h",
            "5h",
            "APP-100",
            "開始日時と作業時間を少ない操作で登録",
            "15分単位の候補が便利",
        ),
        issue(
            "APP-104",
            "バーンダウン",
            "未着手",
            "Sora",
            "2026-09-05",
            "8h",
            "0h",
            "APP-100",
            "任意の親課題以下を対象に残時間を可視化",
            "見積変更も履歴に反映する",
        ),
        issue(
            "OPS-20",
            "リリース準備",
            "未着手",
            "Hiroshi",
            "2026-09-10",
            "10h",
            "0h",
            "",
            "macOSとWindows向けに署名済み成果物を作成",
            "CI構築が必要",
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn issue(
    key: &str,
    summary: &str,
    status: &str,
    assignee: &str,
    due: &str,
    estimate: &str,
    spent: &str,
    parent: &str,
    description: &str,
    comments: &str,
) -> Issue {
    Issue {
        key: key.into(),
        summary: summary.into(),
        status: status.into(),
        assignee: assignee.into(),
        due: due.into(),
        estimate: estimate.into(),
        spent: spent.into(),
        parent: parent.into(),
        description: description.into(),
        comments: comments.into(),
    }
}

fn to_row(issue: &Issue) -> IssueRow {
    IssueRow {
        key: issue.key.clone().into(),
        summary: issue.summary.clone().into(),
        status: issue.status.clone().into(),
        assignee: issue.assignee.clone().into(),
        due: issue.due.clone().into(),
        estimate: issue.estimate.clone().into(),
        spent: issue.spent.clone().into(),
        parent: issue.parent.clone().into(),
    }
}

fn filtered_rows(all: &[Issue], query: &str, parent: &str) -> Vec<IssueRow> {
    let needle = query.to_lowercase();
    all.iter()
        .filter(|item| belongs_to_subtree(item, parent, all))
        .filter(|item| {
            needle.is_empty()
                || [&item.key, &item.summary, &item.description, &item.comments]
                    .iter()
                    .any(|value| value.to_lowercase().contains(&needle))
        })
        .map(to_row)
        .collect()
}

fn belongs_to_subtree(issue: &Issue, root: &str, all: &[Issue]) -> bool {
    if root.is_empty() {
        return true;
    }
    let mut key = issue.key.as_str();
    for _ in 0..=all.len() {
        if key == root {
            return true;
        }
        let Some(current) = all.iter().find(|candidate| candidate.key == key) else {
            return false;
        };
        if current.parent.is_empty() {
            return false;
        }
        key = &current.parent;
    }
    false
}

fn build_tree_nodes(issues: &[Issue]) -> Vec<TreeNode> {
    let keys: HashSet<_> = issues.iter().map(|item| item.key.as_str()).collect();
    let mut children: HashMap<&str, Vec<&Issue>> = HashMap::new();
    for item in issues {
        let parent = if item.parent.is_empty() || !keys.contains(item.parent.as_str()) {
            ""
        } else {
            item.parent.as_str()
        };
        children.entry(parent).or_default().push(item);
    }
    for group in children.values_mut() {
        group.sort_by(|left, right| left.key.cmp(&right.key));
    }

    fn append(
        parent: &str,
        depth: i32,
        children: &HashMap<&str, Vec<&Issue>>,
        visited: &mut HashSet<String>,
        result: &mut Vec<TreeNode>,
    ) {
        let Some(group) = children.get(parent) else {
            return;
        };
        for item in group {
            if !visited.insert(item.key.clone()) {
                continue;
            }
            result.push(TreeNode {
                key: item.key.clone().into(),
                label: format!("{}  {}", item.key, item.summary).into(),
                depth,
            });
            append(&item.key, depth + 1, children, visited, result);
        }
    }

    let mut result = vec![TreeNode {
        key: "".into(),
        label: "▾  すべての課題".into(),
        depth: 0,
    }];
    append("", 0, &children, &mut HashSet::new(), &mut result);
    result
}

fn apply_issue_models(ui: &AppWindow, issues: &[Issue], query: &str, parent: &str) {
    let rows = VecModel::from(filtered_rows(issues, query, parent));
    ui.set_result_count(rows.row_count() as i32);
    ui.set_issues(ModelRc::new(rows));
    ui.set_tree_nodes(ModelRc::new(VecModel::from(build_tree_nodes(issues))));
}

fn select_issue(ui: &AppWindow, item: &Issue) {
    ui.set_selected_key(item.key.clone().into());
    ui.set_selected_summary(item.summary.clone().into());
    ui.set_selected_description(item.description.clone().into());
    ui.set_selected_comments(item.comments.clone().into());
    ui.set_selected_due(item.due.clone().into());
    ui.set_selected_estimate(item.estimate.clone().into());
    ui.set_selected_spent(item.spent.clone().into());
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let (initial_status, initial_issues, has_cache) = match storage::load_latest() {
        Ok(Some((site, items))) => (format!("キャッシュ: {site}"), items, true),
        Ok(None) => ("未接続・デモ表示".into(), demo_issues(), false),
        Err(error) => (format!("キャッシュエラー: {error}"), demo_issues(), false),
    };
    let issues = Arc::new(Mutex::new(initial_issues));
    let current_query = Arc::new(Mutex::new(String::new()));
    let current_parent = Arc::new(Mutex::new(String::new()));
    ui.set_connection_status(initial_status.into());
    ui.set_has_synced_data(has_cache);
    apply_issue_models(&ui, &issues.lock().unwrap(), "", "");

    let weak = ui.as_weak();
    let issues_for_connect = issues.clone();
    let query_for_connect = current_query.clone();
    let parent_for_connect = current_parent.clone();
    ui.on_connect_oauth(move || {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_connection_busy() {
            return;
        }
        ui.set_connection_busy(true);
        ui.set_connection_status("Atlassianへ接続・同期中…".into());
        let weak_for_result = ui.as_weak();
        let issues_for_result = issues_for_connect.clone();
        let query_for_result = query_for_connect.clone();
        let parent_for_result = parent_for_connect.clone();
        std::thread::spawn(move || {
            let result = oauth::connect()
                .and_then(|session| {
                    jira::fetch_all_issues(&session).map_err(oauth::OAuthError::message)
                })
                .map(|(resource, fetched)| {
                    let cache_error = storage::replace_issues(&resource, &fetched).err();
                    (resource, fetched, cache_error)
                });
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak_for_result.upgrade() else {
                    return;
                };
                ui.set_connection_busy(false);
                match result {
                    Ok((resource, fetched, cache_error)) => {
                        *issues_for_result.lock().unwrap() = fetched;
                        query_for_result.lock().unwrap().clear();
                        parent_for_result.lock().unwrap().clear();
                        let guard = issues_for_result.lock().unwrap();
                        apply_issue_models(&ui, &guard, "", "");
                        ui.set_active_parent("すべての課題".into());
                        ui.set_has_synced_data(true);
                        if let Some(first) = guard.first() {
                            select_issue(&ui, first);
                        }
                        let suffix = cache_error
                            .map(|error| format!("・キャッシュ失敗: {error}"))
                            .unwrap_or_default();
                        ui.set_connection_status(
                            format!("接続済み: {}・{}件{suffix}", resource.name, guard.len())
                                .into(),
                        );
                    }
                    Err(error) => ui.set_connection_status(format!("接続エラー: {error}").into()),
                }
            });
        });
    });

    let weak = ui.as_weak();
    let items = issues.clone();
    ui.on_select_issue(move |key| {
        let Some(ui) = weak.upgrade() else { return };
        if let Some(item) = items
            .lock()
            .unwrap()
            .iter()
            .find(|item| item.key == key.as_str())
        {
            select_issue(&ui, item);
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let query_state = current_query.clone();
    let parent_state = current_parent.clone();
    ui.on_search(move |query| {
        *query_state.lock().unwrap() = query.to_string();
        if let Some(ui) = weak.upgrade() {
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query_state.lock().unwrap(),
                &parent_state.lock().unwrap(),
            );
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let query_state = current_query;
    let parent_state = current_parent;
    ui.on_filter_parent(move |parent| {
        *parent_state.lock().unwrap() = parent.to_string();
        if let Some(ui) = weak.upgrade() {
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query_state.lock().unwrap(),
                &parent_state.lock().unwrap(),
            );
            ui.set_active_parent(if parent.is_empty() {
                SharedString::from("すべての課題")
            } else {
                parent
            });
        }
    });

    if let Some(first) = issues.lock().unwrap().first() {
        select_issue(&ui, first);
    }
    ui.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_filter_includes_all_descendants() {
        let items = demo_issues();
        let keys: Vec<_> = filtered_rows(&items, "", "APP-102")
            .into_iter()
            .map(|row| row.key.to_string())
            .collect();
        assert_eq!(keys, ["APP-102", "APP-105"]);
    }

    #[test]
    fn search_covers_description_and_comments() {
        let items = demo_issues();
        assert_eq!(filtered_rows(&items, "キーボード", "").len(), 1);
        assert_eq!(filtered_rows(&items, "仮想リスト", "").len(), 1);
    }

    #[test]
    fn tree_contains_nested_real_issue_models() {
        let tree = build_tree_nodes(&demo_issues());
        let nested = tree
            .iter()
            .find(|node| node.key.as_str() == "APP-105")
            .unwrap();
        assert_eq!(nested.depth, 2);
    }
}
