mod jira;
mod oauth;
mod storage;

use chrono::{Datelike, Local, NaiveDate};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

slint::include_modules!();

#[derive(Clone, Debug)]
struct Issue {
    key: String,
    summary: String,
    status: String,
    status_done: bool,
    issue_type: String,
    assignee: String,
    due: String,
    estimate: String,
    spent: String,
    remaining: String,
    estimate_seconds: i64,
    spent_seconds: i64,
    remaining_seconds: i64,
    parent: String,
    description: String,
    comments: String,
    custom_values: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CustomField {
    id: String,
    name: String,
    field_type: String,
    editable: bool,
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
        status_done: status == "完了",
        issue_type: if parent.is_empty() {
            "エピック"
        } else {
            "タスク"
        }
        .into(),
        assignee: assignee.into(),
        due: due.into(),
        estimate: estimate.into(),
        spent: spent.into(),
        remaining: estimate.into(),
        estimate_seconds: parse_duration(estimate).unwrap_or(0),
        spent_seconds: parse_duration(spent).unwrap_or(0),
        remaining_seconds: parse_duration(estimate).unwrap_or(0),
        parent: parent.into(),
        description: description.into(),
        comments: comments.into(),
        custom_values: HashMap::new(),
    }
}

fn parse_duration(value: &str) -> Result<i64, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    let mut seconds = 0_i64;
    for part in trimmed.split_whitespace() {
        let (number, unit) = part.split_at(part.len().saturating_sub(1));
        let amount = number
            .parse::<i64>()
            .map_err(|_| "時間は 2h 30m の形式で入力してください。".to_owned())?;
        if amount < 0 {
            return Err("時間に負の値は指定できません。".into());
        }
        seconds = seconds
            .checked_add(match unit {
                "d" => amount.saturating_mul(8 * 3600),
                "h" => amount.saturating_mul(3600),
                "m" => amount.saturating_mul(60),
                _ => return Err("時間の単位は d、h、m を使用してください。".into()),
            })
            .ok_or_else(|| "時間が大きすぎます。".to_owned())?;
    }
    Ok(seconds)
}

fn issue_depth(issue: &Issue, all: &[Issue]) -> i32 {
    let mut depth = 0;
    let mut parent = issue.parent.as_str();
    let mut visited = HashSet::new();
    while !parent.is_empty() && visited.insert(parent) && depth < all.len() as i32 {
        depth += 1;
        parent = all
            .iter()
            .find(|candidate| candidate.key == parent)
            .map(|candidate| candidate.parent.as_str())
            .unwrap_or("");
    }
    depth
}

fn to_row(
    issue: &Issue,
    all: &[Issue],
    visible_custom_fields: &[CustomField],
    collapsed: &HashSet<String>,
) -> IssueRow {
    IssueRow {
        key: issue.key.clone().into(),
        summary: issue.summary.clone().into(),
        status: issue.status.clone().into(),
        status_done: issue.status_done,
        issue_type: issue.issue_type.clone().into(),
        assignee: issue.assignee.clone().into(),
        due: issue.due.clone().into(),
        estimate: issue.estimate.clone().into(),
        spent: issue.spent.clone().into(),
        remaining: issue.remaining.clone().into(),
        parent: issue.parent.clone().into(),
        depth: issue_depth(issue, all),
        has_children: all.iter().any(|child| child.parent == issue.key),
        expanded: !collapsed.contains(&issue.key),
        custom_cells: ModelRc::new(VecModel::from(
            visible_custom_fields
                .iter()
                .map(|field| CustomCell {
                    field_id: field.id.clone().into(),
                    value: issue
                        .custom_values
                        .get(&field.id)
                        .cloned()
                        .unwrap_or_default()
                        .into(),
                    editable: field.editable,
                    boolean: field.field_type == "boolean",
                })
                .collect::<Vec<_>>(),
        )),
    }
}

fn filtered_rows(
    all: &[Issue],
    query: &str,
    parent: &str,
    issue_type: &str,
    visible_custom_fields: &[CustomField],
    status_filter: &str,
    collapsed: &HashSet<String>,
) -> Vec<IssueRow> {
    let needle = query.to_lowercase();
    hierarchical_issues(all)
        .into_iter()
        .filter(|item| belongs_to_subtree(item, parent, all))
        .filter(|item| issue_type.is_empty() || item.issue_type == issue_type)
        .filter(|item| {
            status_filter.is_empty()
                || status_filter == "すべての状態"
                || (status_filter == "完了以外" && !item.status_done)
                || item.status == status_filter
        })
        .filter(|item| !hidden_by_collapsed(item, all, collapsed))
        .filter(|item| {
            needle.is_empty()
                || [&item.key, &item.summary, &item.description, &item.comments]
                    .iter()
                    .any(|value| value.to_lowercase().contains(&needle))
        })
        .map(|issue| to_row(issue, all, visible_custom_fields, collapsed))
        .collect()
}

fn hidden_by_collapsed(issue: &Issue, all: &[Issue], collapsed: &HashSet<String>) -> bool {
    let mut parent = issue.parent.as_str();
    let mut visited = HashSet::new();
    while !parent.is_empty() && visited.insert(parent) {
        if collapsed.contains(parent) {
            return true;
        }
        parent = all
            .iter()
            .find(|candidate| candidate.key == parent)
            .map(|candidate| candidate.parent.as_str())
            .unwrap_or("");
    }
    false
}

fn hierarchical_issues(all: &[Issue]) -> Vec<&Issue> {
    let keys = all
        .iter()
        .map(|issue| issue.key.as_str())
        .collect::<HashSet<_>>();
    let mut children: HashMap<&str, Vec<&Issue>> = HashMap::new();
    for issue in all {
        let parent = if issue.parent.is_empty() || !keys.contains(issue.parent.as_str()) {
            ""
        } else {
            issue.parent.as_str()
        };
        children.entry(parent).or_default().push(issue);
    }
    for group in children.values_mut() {
        group.sort_by(|left, right| left.key.cmp(&right.key));
    }
    fn append<'a>(
        parent: &str,
        children: &HashMap<&str, Vec<&'a Issue>>,
        visited: &mut HashSet<String>,
        result: &mut Vec<&'a Issue>,
    ) {
        if let Some(group) = children.get(parent) {
            for issue in group {
                if visited.insert(issue.key.clone()) {
                    result.push(issue);
                    append(&issue.key, children, visited, result);
                }
            }
        }
    }
    let mut result = Vec::with_capacity(all.len());
    let mut visited = HashSet::new();
    append("", &children, &mut visited, &mut result);
    let mut remainder = all
        .iter()
        .filter(|issue| !visited.contains(&issue.key))
        .collect::<Vec<_>>();
    remainder.sort_by(|left, right| left.key.cmp(&right.key));
    result.extend(remainder);
    result
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

fn build_tree_nodes(
    issues: &[Issue],
    issue_type: &str,
    collapsed: &HashSet<String>,
    favorites: &HashSet<String>,
) -> Vec<TreeNode> {
    let keys: HashSet<_> = issues.iter().map(|item| item.key.as_str()).collect();
    let mut included: HashSet<&str> = HashSet::new();
    for item in issues
        .iter()
        .filter(|item| issue_type.is_empty() || item.issue_type == issue_type)
    {
        let mut key = item.key.as_str();
        for _ in 0..=issues.len() {
            if !included.insert(key) {
                break;
            }
            let Some(current) = issues.iter().find(|candidate| candidate.key == key) else {
                break;
            };
            if current.parent.is_empty() {
                break;
            }
            key = &current.parent;
        }
    }
    if !issue_type.is_empty() {
        let mut changed = true;
        while changed {
            changed = false;
            for item in issues {
                if included.contains(item.parent.as_str()) && included.insert(item.key.as_str()) {
                    changed = true;
                }
            }
        }
    }
    let mut children: HashMap<&str, Vec<&Issue>> = HashMap::new();
    for item in issues
        .iter()
        .filter(|item| included.contains(item.key.as_str()))
    {
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

    #[allow(clippy::too_many_arguments)]
    fn append(
        parent: &str,
        depth: i32,
        children: &HashMap<&str, Vec<&Issue>>,
        collapsed: &HashSet<String>,
        visited: &mut HashSet<String>,
        result: &mut Vec<TreeNode>,
        favorites: &HashSet<String>,
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
                has_children: children.contains_key(item.key.as_str()),
                expanded: !collapsed.contains(&item.key),
                favorite: favorites.contains(&item.key),
            });
            if !collapsed.contains(&item.key) {
                append(
                    &item.key,
                    depth + 1,
                    children,
                    collapsed,
                    visited,
                    result,
                    favorites,
                );
            }
        }
    }

    let mut result = favorites
        .iter()
        .filter_map(|key| issues.iter().find(|issue| &issue.key == key))
        .map(|item| TreeNode {
            key: item.key.clone().into(),
            label: format!("固定  {}  {}", item.key, item.summary).into(),
            depth: 0,
            has_children: false,
            expanded: true,
            favorite: true,
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| left.key.cmp(&right.key));
    append(
        "",
        0,
        &children,
        collapsed,
        &mut HashSet::new(),
        &mut result,
        favorites,
    );
    result
}

fn apply_issue_models(
    ui: &AppWindow,
    issues: &[Issue],
    query: &str,
    parent: &str,
    issue_type: &str,
    collapsed: &HashSet<String>,
) {
    let favorites = ui
        .get_tree_nodes()
        .iter()
        .filter(|node| node.favorite && !node.key.is_empty())
        .map(|node| node.key.to_string())
        .collect::<HashSet<_>>();
    let visible_custom_fields = ui
        .get_custom_columns()
        .iter()
        .map(|column| CustomField {
            id: column.id.to_string(),
            name: column.name.to_string(),
            field_type: String::new(),
            editable: column.editable,
        })
        .collect::<Vec<_>>();
    let rows = VecModel::from(filtered_rows(
        issues,
        query,
        parent,
        issue_type,
        &visible_custom_fields,
        ui.get_active_status_filter().as_str(),
        collapsed,
    ));
    ui.set_result_count(rows.row_count() as i32);
    ui.set_issues(ModelRc::new(rows));
    ui.set_tree_nodes(ModelRc::new(VecModel::from(build_tree_nodes(
        issues, issue_type, collapsed, &favorites,
    ))));
    let mut types = issues
        .iter()
        .map(|issue| issue.issue_type.clone())
        .collect::<Vec<_>>();
    types.sort();
    types.dedup();
    types.insert(0, "すべてのタイプ".into());
    let selected_type_index = types
        .iter()
        .position(|value| value == issue_type)
        .unwrap_or(0) as i32;
    ui.set_issue_types(ModelRc::new(VecModel::from(
        types
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    ui.set_selected_type_index(selected_type_index);
    let mut statuses = issues
        .iter()
        .map(|issue| issue.status.clone())
        .collect::<Vec<_>>();
    statuses.sort();
    statuses.dedup();
    statuses.insert(0, "完了以外".into());
    statuses.insert(0, "すべての状態".into());
    let selected = statuses
        .iter()
        .position(|status| status == ui.get_active_status_filter().as_str())
        .unwrap_or(0) as i32;
    ui.set_status_filters(ModelRc::new(VecModel::from(
        statuses
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    ui.set_selected_status_index(selected);
}

fn apply_custom_field_models(
    ui: &AppWindow,
    custom_fields: &[CustomField],
    visible_ids: &[String],
) {
    ui.set_custom_columns(ModelRc::new(VecModel::from(
        visible_ids
            .iter()
            .filter_map(|id| custom_fields.iter().find(|field| &field.id == id))
            .map(|field| CustomColumn {
                id: field.id.clone().into(),
                name: field.name.clone().into(),
                editable: field.editable,
            })
            .collect::<Vec<_>>(),
    )));
    ui.set_available_custom_fields(ModelRc::new(VecModel::from(
        std::iter::once(SharedString::from("項目を追加..."))
            .chain(
                custom_fields
                    .iter()
                    .filter(|field| !visible_ids.contains(&field.id))
                    .map(|field| SharedString::from(field.name.clone())),
            )
            .collect::<Vec<_>>(),
    )));
    ui.set_selected_custom_field_index(0);
}

fn select_issue(ui: &AppWindow, item: &Issue) {
    ui.set_selected_key(item.key.clone().into());
    ui.set_selected_summary(item.summary.clone().into());
    ui.set_selected_description(item.description.clone().into());
    ui.set_selected_comments(item.comments.clone().into());
    ui.set_selected_due(item.due.clone().into());
    ui.set_selected_estimate(item.estimate.clone().into());
    ui.set_selected_spent(item.spent.clone().into());
    ui.set_selected_remaining(item.remaining.clone().into());
    ui.set_worklog_remaining(item.remaining.clone().into());
}

#[derive(Clone)]
struct CalendarState {
    year: i32,
    month: u32,
}

impl CalendarState {
    fn today() -> Self {
        let today = Local::now().date_naive();
        Self {
            year: today.year(),
            month: today.month(),
        }
    }

    fn move_month(&mut self, amount: i32) {
        let index = self.year * 12 + self.month as i32 - 1 + amount;
        self.year = index.div_euclid(12);
        self.month = index.rem_euclid(12) as u32 + 1;
    }
}

fn apply_calendar(ui: &AppWindow, state: &CalendarState) {
    let first = NaiveDate::from_ymd_opt(state.year, state.month, 1).unwrap();
    let start = first - chrono::Days::new(first.weekday().num_days_from_sunday().into());
    let days = (0..42)
        .map(|index| {
            let date = start + chrono::Days::new(index);
            CalendarDay {
                label: date.day().to_string().into(),
                date: date.format("%Y-%m-%d").to_string().into(),
                row: (index / 7) as i32,
                column: (index % 7) as i32,
                current_month: date.month() == state.month,
            }
        })
        .collect::<Vec<_>>();
    ui.set_calendar_label(format!("{}年 {}月", state.year, state.month).into());
    ui.set_calendar_days(ModelRc::new(VecModel::from(days)));
}

fn validate_due(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").is_ok() {
        Ok(())
    } else {
        Err("期限はカレンダーから選択するか YYYY-MM-DD で指定してください。".into())
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let (initial_status, initial_issues, initial_resource, initial_custom_fields, has_cache) =
        match storage::load_latest() {
            Ok(Some((resource, items, fields))) => (
                format!("キャッシュ: {}", resource.name),
                items,
                Some(resource),
                fields,
                true,
            ),
            Ok(None) => (
                "未接続・デモ表示".into(),
                demo_issues(),
                None,
                Vec::new(),
                false,
            ),
            Err(error) => (
                format!("キャッシュエラー: {error}"),
                demo_issues(),
                None,
                Vec::new(),
                false,
            ),
        };
    let issues = Arc::new(Mutex::new(initial_issues));
    let current_resource = Arc::new(Mutex::new(initial_resource));
    let custom_fields = Arc::new(Mutex::new(initial_custom_fields));
    let visible_custom_ids = Arc::new(Mutex::new(
        storage::load_visible_custom_columns().unwrap_or_default(),
    ));
    let favorite_nodes = Arc::new(Mutex::new(
        current_resource
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|resource| storage::load_favorites(&resource.id).ok())
            .unwrap_or_default(),
    ));
    let edit_draft = Arc::new(Mutex::new(None::<HashMap<String, Issue>>));
    let active_session = Arc::new(Mutex::new(None::<oauth::SavedSession>));
    let current_query = Arc::new(Mutex::new(String::new()));
    let current_parent = Arc::new(Mutex::new(String::new()));
    let current_type = Arc::new(Mutex::new(String::new()));
    let collapsed_nodes = Arc::new(Mutex::new(HashSet::<String>::new()));
    let calendar_state = Arc::new(Mutex::new(CalendarState::today()));
    let detail_windows = Rc::new(RefCell::new(Vec::<IssueDetailWindow>::new()));

    ui.set_connection_status(initial_status.into());
    ui.set_has_synced_data(has_cache);
    if let Ok((assignee, due, estimate, spent, remaining)) = storage::load_column_settings() {
        ui.set_show_assignee(assignee);
        ui.set_show_due(due);
        ui.set_show_estimate(estimate);
        ui.set_show_spent(spent);
        ui.set_show_remaining(remaining);
    }
    ui.set_worklog_date(
        Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string()
            .into(),
    );
    apply_custom_field_models(
        &ui,
        &custom_fields.lock().unwrap(),
        &visible_custom_ids.lock().unwrap(),
    );
    apply_issue_models(
        &ui,
        &issues.lock().unwrap(),
        "",
        "",
        "",
        &collapsed_nodes.lock().unwrap(),
    );
    ui.set_tree_nodes(ModelRc::new(VecModel::from(build_tree_nodes(
        &issues.lock().unwrap(),
        "",
        &collapsed_nodes.lock().unwrap(),
        &favorite_nodes.lock().unwrap(),
    ))));
    apply_calendar(&ui, &calendar_state.lock().unwrap());

    let weak = ui.as_weak();
    let issues_for_connect = issues.clone();
    let resource_for_connect = current_resource.clone();
    let fields_for_connect = custom_fields.clone();
    let favorites_for_connect = favorite_nodes.clone();
    let visible_for_connect = visible_custom_ids.clone();
    let session_for_connect = active_session.clone();
    let query_for_connect = current_query.clone();
    let parent_for_connect = current_parent.clone();
    let type_for_connect = current_type.clone();
    let collapsed_for_connect = collapsed_nodes.clone();
    ui.on_connect_oauth(move || {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_connection_busy() {
            return;
        }
        ui.set_connection_busy(true);
        ui.set_connection_status("Atlassianへ接続・同期中...".into());
        let weak_for_result = ui.as_weak();
        let issues_for_result = issues_for_connect.clone();
        let resource_for_result = resource_for_connect.clone();
        let fields_for_result = fields_for_connect.clone();
        let favorites_for_result = favorites_for_connect.clone();
        let visible_for_result = visible_for_connect.clone();
        let session_for_result = session_for_connect.clone();
        let query_for_result = query_for_connect.clone();
        let parent_for_result = parent_for_connect.clone();
        let type_for_result = type_for_connect.clone();
        let collapsed_for_result = collapsed_for_connect.clone();
        std::thread::spawn(move || {
            let result = oauth::connect().and_then(|session| {
                jira::fetch_all_issues(&session)
                    .map_err(oauth::OAuthError::message)
                    .map(|(resource, fetched, fields)| {
                        let cache_error =
                            storage::replace_issues(&resource, &fetched, &fields).err();
                        (session, resource, fetched, fields, cache_error)
                    })
            });
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak_for_result.upgrade() else {
                    return;
                };
                ui.set_connection_busy(false);
                match result {
                    Ok((session, resource, fetched, fields, cache_error)) => {
                        *session_for_result.lock().unwrap() = Some(session);
                        *resource_for_result.lock().unwrap() = Some(resource.clone());
                        *issues_for_result.lock().unwrap() = fetched;
                        *fields_for_result.lock().unwrap() = fields;
                        *favorites_for_result.lock().unwrap() =
                            storage::load_favorites(&resource.id).unwrap_or_default();
                        query_for_result.lock().unwrap().clear();
                        parent_for_result.lock().unwrap().clear();
                        type_for_result.lock().unwrap().clear();
                        collapsed_for_result.lock().unwrap().clear();
                        let guard = issues_for_result.lock().unwrap();
                        apply_custom_field_models(
                            &ui,
                            &fields_for_result.lock().unwrap(),
                            &visible_for_result.lock().unwrap(),
                        );
                        apply_issue_models(
                            &ui,
                            &guard,
                            "",
                            "",
                            "",
                            &collapsed_for_result.lock().unwrap(),
                        );
                        ui.set_tree_nodes(ModelRc::new(VecModel::from(build_tree_nodes(
                            &guard,
                            "",
                            &collapsed_for_result.lock().unwrap(),
                            &favorites_for_result.lock().unwrap(),
                        ))));
                        ui.set_active_parent("すべての課題".into());
                        ui.set_has_synced_data(true);
                        ui.set_action_status("".into());
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
            ui.set_action_status("".into());
        }
    });

    let weak = ui.as_weak();
    let items = issues.clone();
    let windows = detail_windows;
    ui.on_open_selected_issue(move || {
        let Some(ui) = weak.upgrade() else { return };
        let selected_key = ui.get_selected_key();
        let guard = items.lock().unwrap();
        let Some(item) = guard.iter().find(|item| item.key == selected_key.as_str()) else {
            return;
        };
        let Ok(window) = IssueDetailWindow::new() else {
            ui.set_action_status("課題ウィンドウを作成できませんでした。".into());
            return;
        };
        window.set_issue_key(item.key.clone().into());
        window.set_summary(item.summary.clone().into());
        window.set_issue_type(item.issue_type.clone().into());
        window.set_status(item.status.clone().into());
        window.set_assignee(item.assignee.clone().into());
        window.set_due(item.due.clone().into());
        window.set_estimate(item.estimate.clone().into());
        window.set_spent(item.spent.clone().into());
        window.set_remaining(item.remaining.clone().into());
        window.set_description(item.description.clone().into());
        window.set_comments(item.comments.clone().into());
        if window.show().is_ok() {
            windows.borrow_mut().push(window);
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let query_state = current_query.clone();
    let parent_state = current_parent.clone();
    let type_state = current_type.clone();
    let collapsed_state = collapsed_nodes.clone();
    ui.on_search(move |query| {
        *query_state.lock().unwrap() = query.to_string();
        if let Some(ui) = weak.upgrade() {
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query_state.lock().unwrap(),
                &parent_state.lock().unwrap(),
                &type_state.lock().unwrap(),
                &collapsed_state.lock().unwrap(),
            );
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let query_state = current_query.clone();
    let parent_state = current_parent.clone();
    let type_state = current_type.clone();
    let collapsed_state = collapsed_nodes.clone();
    ui.on_filter_parent(move |parent| {
        *parent_state.lock().unwrap() = parent.to_string();
        if let Some(ui) = weak.upgrade() {
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query_state.lock().unwrap(),
                &parent_state.lock().unwrap(),
                &type_state.lock().unwrap(),
                &collapsed_state.lock().unwrap(),
            );
            ui.set_active_parent(if parent.is_empty() {
                SharedString::from("すべての課題")
            } else {
                parent
            });
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let query_state = current_query.clone();
    let parent_state = current_parent.clone();
    let type_state = current_type.clone();
    let collapsed_state = collapsed_nodes.clone();
    ui.on_filter_type(move |selected| {
        *type_state.lock().unwrap() = if selected == "すべてのタイプ" {
            String::new()
        } else {
            selected.to_string()
        };
        if let Some(ui) = weak.upgrade() {
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query_state.lock().unwrap(),
                &parent_state.lock().unwrap(),
                &type_state.lock().unwrap(),
                &collapsed_state.lock().unwrap(),
            );
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let query_state = current_query.clone();
    let parent_state = current_parent.clone();
    let type_state = current_type.clone();
    let collapsed_state = collapsed_nodes.clone();
    ui.on_toggle_tree(move |key| {
        let mut collapsed = collapsed_state.lock().unwrap();
        if !collapsed.insert(key.to_string()) {
            collapsed.remove(key.as_str());
        }
        if let Some(ui) = weak.upgrade() {
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query_state.lock().unwrap(),
                &parent_state.lock().unwrap(),
                &type_state.lock().unwrap(),
                &collapsed,
            );
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let query = current_query.clone();
    let parent = current_parent.clone();
    let issue_type = current_type.clone();
    let collapsed = collapsed_nodes.clone();
    ui.on_expand_all_tree(move || {
        collapsed.lock().unwrap().clear();
        if let Some(ui) = weak.upgrade() {
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query.lock().unwrap(),
                &parent.lock().unwrap(),
                &issue_type.lock().unwrap(),
                &collapsed.lock().unwrap(),
            );
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let query = current_query.clone();
    let parent = current_parent.clone();
    let issue_type = current_type.clone();
    let collapsed = collapsed_nodes.clone();
    ui.on_collapse_all_tree(move || {
        let parent_keys = {
            let guard = all.lock().unwrap();
            guard
                .iter()
                .filter(|issue| guard.iter().any(|child| child.parent == issue.key))
                .map(|issue| issue.key.clone())
                .collect()
        };
        *collapsed.lock().unwrap() = parent_keys;
        if let Some(ui) = weak.upgrade() {
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query.lock().unwrap(),
                &parent.lock().unwrap(),
                &issue_type.lock().unwrap(),
                &collapsed.lock().unwrap(),
            );
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let query = current_query.clone();
    let parent = current_parent.clone();
    let issue_type = current_type.clone();
    let collapsed = collapsed_nodes.clone();
    ui.on_filter_status(move |_| {
        if let Some(ui) = weak.upgrade() {
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query.lock().unwrap(),
                &parent.lock().unwrap(),
                &issue_type.lock().unwrap(),
                &collapsed.lock().unwrap(),
            );
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let resource = current_resource.clone();
    let favorites = favorite_nodes.clone();
    let type_state = current_type.clone();
    let collapsed_state = collapsed_nodes.clone();
    ui.on_toggle_favorite(move |key| {
        let key = key.to_string();
        let mut values = favorites.lock().unwrap();
        let enabled = if values.insert(key.clone()) {
            true
        } else {
            values.remove(&key);
            false
        };
        if let Some(resource) = resource.lock().unwrap().as_ref()
            && let Err(error) = storage::set_favorite(&resource.id, &key, enabled)
            && let Some(ui) = weak.upgrade()
        {
            ui.set_action_status(error.into());
        }
        if let Some(ui) = weak.upgrade() {
            ui.set_tree_nodes(ModelRc::new(VecModel::from(build_tree_nodes(
                &all.lock().unwrap(),
                &type_state.lock().unwrap(),
                &collapsed_state.lock().unwrap(),
                &values,
            ))));
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let fields = custom_fields.clone();
    let visible = visible_custom_ids.clone();
    let query = current_query.clone();
    let parent = current_parent.clone();
    let issue_type = current_type.clone();
    let collapsed = collapsed_nodes.clone();
    ui.on_add_custom_column(move |name| {
        if name == "項目を追加..." {
            return;
        }
        let fields_guard = fields.lock().unwrap();
        let Some(field) = fields_guard
            .iter()
            .find(|field| field.name == name.as_str())
        else {
            return;
        };
        let mut visible_guard = visible.lock().unwrap();
        if !visible_guard.contains(&field.id) {
            visible_guard.push(field.id.clone());
        }
        if let Some(ui) = weak.upgrade() {
            if let Err(error) = storage::save_visible_custom_columns(&visible_guard) {
                ui.set_action_status(error.into());
            }
            apply_custom_field_models(&ui, &fields_guard, &visible_guard);
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query.lock().unwrap(),
                &parent.lock().unwrap(),
                &issue_type.lock().unwrap(),
                &collapsed.lock().unwrap(),
            );
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let fields = custom_fields.clone();
    let visible = visible_custom_ids.clone();
    let query = current_query.clone();
    let parent = current_parent.clone();
    let issue_type = current_type.clone();
    let collapsed = collapsed_nodes.clone();
    ui.on_remove_custom_column(move |id| {
        let mut visible_guard = visible.lock().unwrap();
        visible_guard.retain(|value| value != id.as_str());
        if let Some(ui) = weak.upgrade() {
            if let Err(error) = storage::save_visible_custom_columns(&visible_guard) {
                ui.set_action_status(error.into());
            }
            apply_custom_field_models(&ui, &fields.lock().unwrap(), &visible_guard);
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query.lock().unwrap(),
                &parent.lock().unwrap(),
                &issue_type.lock().unwrap(),
                &collapsed.lock().unwrap(),
            );
        }
    });

    let draft = edit_draft.clone();
    let all = issues.clone();
    ui.on_begin_table_edit(move || {
        *draft.lock().unwrap() = Some(
            all.lock()
                .unwrap()
                .iter()
                .cloned()
                .map(|issue| (issue.key.clone(), issue))
                .collect(),
        );
    });

    let draft = edit_draft.clone();
    ui.on_edit_table_cell(move |key, field, value| {
        let mut guard = draft.lock().unwrap();
        let Some(issue) = guard.as_mut().and_then(|items| items.get_mut(key.as_str())) else {
            return;
        };
        match field.as_str() {
            "summary" => issue.summary = value.to_string(),
            "duedate" => issue.due = value.to_string(),
            "estimate" => issue.estimate = value.to_string(),
            "remaining" => issue.remaining = value.to_string(),
            id => {
                issue.custom_values.insert(id.to_owned(), value.to_string());
            }
        }
    });

    let weak = ui.as_weak();
    let draft = edit_draft.clone();
    let all = issues.clone();
    let query = current_query.clone();
    let parent = current_parent.clone();
    let issue_type = current_type.clone();
    let collapsed = collapsed_nodes.clone();
    ui.on_discard_table_edit(move || {
        *draft.lock().unwrap() = None;
        if let Some(ui) = weak.upgrade() {
            ui.set_edit_mode(false);
            apply_issue_models(
                &ui,
                &all.lock().unwrap(),
                &query.lock().unwrap(),
                &parent.lock().unwrap(),
                &issue_type.lock().unwrap(),
                &collapsed.lock().unwrap(),
            );
            ui.set_action_status("一覧の変更を破棄しました。".into());
        }
    });

    let weak = ui.as_weak();
    let draft = edit_draft.clone();
    let all = issues.clone();
    let fields = custom_fields.clone();
    let visible = visible_custom_ids.clone();
    let session = active_session.clone();
    let resource = current_resource.clone();
    let query = current_query.clone();
    let parent = current_parent.clone();
    let issue_type = current_type.clone();
    let collapsed = collapsed_nodes.clone();
    ui.on_save_table_edit(move || {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_action_busy() {
            return;
        }
        let Some(session_value) = session.lock().unwrap().clone() else {
            ui.set_action_status("編集内容の保存前に「再同期」でJiraへ接続してください。".into());
            return;
        };
        let originals = all.lock().unwrap().clone();
        let Some(draft_values) = draft.lock().unwrap().clone() else {
            return;
        };
        let changed = originals
            .iter()
            .filter_map(|original| {
                let updated = draft_values.get(&original.key)?;
                if original.summary != updated.summary
                    || original.due != updated.due
                    || original.estimate != updated.estimate
                    || original.remaining != updated.remaining
                    || original.custom_values != updated.custom_values
                {
                    Some(updated.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for issue in &changed {
            if issue.summary.trim().is_empty() {
                ui.set_action_status(format!("{}: タイトルは空にできません。", issue.key).into());
                return;
            }
            if let Err(error) = validate_due(&issue.due) {
                ui.set_action_status(format!("{}: {error}", issue.key).into());
                return;
            }
            if let Err(error) = parse_duration(&issue.estimate) {
                ui.set_action_status(format!("{}: {error}", issue.key).into());
                return;
            }
            if let Err(error) = parse_duration(&issue.remaining) {
                ui.set_action_status(format!("{}: 残余時間: {error}", issue.key).into());
                return;
            }
        }
        if changed.is_empty() {
            *draft.lock().unwrap() = None;
            ui.set_edit_mode(false);
            ui.set_action_status("変更はありません。".into());
            return;
        }
        ui.set_action_busy(true);
        ui.set_action_status(format!("{}件の課題を保存中...", changed.len()).into());
        let weak_result = ui.as_weak();
        let all_result = all.clone();
        let fields_result = fields.clone();
        let visible_result = visible.clone();
        let resource_result = resource.clone();
        let draft_result = draft.clone();
        let query_result = query.clone();
        let parent_result = parent.clone();
        let type_result = issue_type.clone();
        let collapsed_result = collapsed.clone();
        let field_defs = fields.lock().unwrap().clone();
        std::thread::spawn(move || {
            let result = changed
                .iter()
                .try_for_each(|issue| {
                    jira::update_issue_fields(
                        &session_value,
                        &issue.key,
                        &issue.summary,
                        &issue.due,
                        parse_duration(&issue.estimate)?,
                        parse_duration(&issue.remaining)?,
                        &issue.custom_values,
                        &field_defs,
                    )
                })
                .and_then(|()| jira::fetch_all_issues(&session_value))
                .map(|(resource, fetched, fields)| {
                    let cache_error = storage::replace_issues(&resource, &fetched, &fields).err();
                    (resource, fetched, fields, cache_error)
                });
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak_result.upgrade() else {
                    return;
                };
                ui.set_action_busy(false);
                match result {
                    Ok((new_resource, fetched, new_fields, cache_error)) => {
                        *all_result.lock().unwrap() = fetched;
                        *fields_result.lock().unwrap() = new_fields;
                        *resource_result.lock().unwrap() = Some(new_resource);
                        *draft_result.lock().unwrap() = None;
                        ui.set_edit_mode(false);
                        apply_custom_field_models(
                            &ui,
                            &fields_result.lock().unwrap(),
                            &visible_result.lock().unwrap(),
                        );
                        apply_issue_models(
                            &ui,
                            &all_result.lock().unwrap(),
                            &query_result.lock().unwrap(),
                            &parent_result.lock().unwrap(),
                            &type_result.lock().unwrap(),
                            &collapsed_result.lock().unwrap(),
                        );
                        ui.set_action_status(
                            cache_error
                                .map(|error| format!("Jira更新済み・キャッシュ失敗: {error}"))
                                .unwrap_or_else(|| "一覧の変更を保存しました。".into())
                                .into(),
                        );
                    }
                    Err(error) => ui.set_action_status(format!("一覧の保存エラー: {error}").into()),
                }
            });
        });
    });

    let weak = ui.as_weak();
    let calendar = calendar_state.clone();
    ui.on_calendar_previous(move || {
        if let Some(ui) = weak.upgrade() {
            calendar.lock().unwrap().move_month(-1);
            apply_calendar(&ui, &calendar.lock().unwrap());
        }
    });

    let weak = ui.as_weak();
    let calendar = calendar_state;
    ui.on_calendar_next(move || {
        if let Some(ui) = weak.upgrade() {
            calendar.lock().unwrap().move_month(1);
            apply_calendar(&ui, &calendar.lock().unwrap());
        }
    });

    let weak = ui.as_weak();
    ui.on_choose_date(move |date| {
        if let Some(ui) = weak.upgrade() {
            if ui.get_calendar_target() == "worklog" {
                ui.set_worklog_date(date);
            } else {
                ui.set_selected_due(date);
            }
            ui.set_show_calendar(false);
        }
    });

    let weak = ui.as_weak();
    ui.on_columns_changed(move |assignee, due, estimate, spent, remaining| {
        if let Err(error) =
            storage::save_column_settings((assignee, due, estimate, spent, remaining))
            && let Some(ui) = weak.upgrade()
        {
            ui.set_action_status(error.into());
        }
    });

    let weak = ui.as_weak();
    let all_for_save = issues.clone();
    let resource_for_save = current_resource.clone();
    let fields_for_save = custom_fields.clone();
    let visible_for_save = visible_custom_ids.clone();
    let session_for_save = active_session.clone();
    let query_for_save = current_query.clone();
    let parent_for_save = current_parent.clone();
    let type_for_save = current_type.clone();
    let collapsed_for_save = collapsed_nodes.clone();
    ui.on_save_issue(move |key, summary, description, due, estimate| {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_action_busy() {
            return;
        }
        let estimate_seconds = match parse_duration(estimate.as_str()) {
            Ok(value) => value,
            Err(error) => {
                ui.set_action_status(error.into());
                return;
            }
        };
        if let Err(error) = validate_due(due.as_str()) {
            ui.set_action_status(error.into());
            return;
        }
        let Some(session) = session_for_save.lock().unwrap().clone() else {
            ui.set_action_status("編集前に「再同期」でJiraへ接続してください。".into());
            return;
        };
        if summary.trim().is_empty() {
            ui.set_action_status("タイトルは空にできません。".into());
            return;
        }
        ui.set_action_busy(true);
        ui.set_action_status("課題を更新中...".into());
        let weak_for_result = ui.as_weak();
        let all_for_result = all_for_save.clone();
        let resource_for_result = resource_for_save.clone();
        let fields_for_result = fields_for_save.clone();
        let visible_for_result = visible_for_save.clone();
        let query_for_result = query_for_save.clone();
        let parent_for_result = parent_for_save.clone();
        let type_for_result = type_for_save.clone();
        let collapsed_for_result = collapsed_for_save.clone();
        let key = key.to_string();
        let due = due.to_string();
        std::thread::spawn(move || {
            let result = jira::update_issue(
                &session,
                &key,
                summary.as_str(),
                description.as_str(),
                &due,
                estimate_seconds,
            )
            .and_then(|()| jira::fetch_all_issues(&session))
            .map(|(resource, fetched, fields)| {
                let cache_error = storage::replace_issues(&resource, &fetched, &fields).err();
                (resource, fetched, fields, cache_error)
            });
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak_for_result.upgrade() else {
                    return;
                };
                ui.set_action_busy(false);
                match result {
                    Ok((resource, fetched, fields, cache_error)) => {
                        *resource_for_result.lock().unwrap() = Some(resource.clone());
                        *all_for_result.lock().unwrap() = fetched;
                        *fields_for_result.lock().unwrap() = fields;
                        let guard = all_for_result.lock().unwrap();
                        let query = query_for_result.lock().unwrap().clone();
                        let parent = parent_for_result.lock().unwrap().clone();
                        let issue_type = type_for_result.lock().unwrap().clone();
                        apply_custom_field_models(
                            &ui,
                            &fields_for_result.lock().unwrap(),
                            &visible_for_result.lock().unwrap(),
                        );
                        apply_issue_models(
                            &ui,
                            &guard,
                            &query,
                            &parent,
                            &issue_type,
                            &collapsed_for_result.lock().unwrap(),
                        );
                        if let Some(item) = guard.iter().find(|item| item.key == key) {
                            select_issue(&ui, item);
                        }
                        ui.set_action_status(
                            cache_error
                                .map(|error| format!("Jira更新済み・キャッシュ失敗: {error}"))
                                .unwrap_or_else(|| "課題を更新しました。".into())
                                .into(),
                        );
                    }
                    Err(error) => ui.set_action_status(format!("更新エラー: {error}").into()),
                }
            });
        });
    });

    let weak = ui.as_weak();
    let all_for_worklog = issues.clone();
    let resource_for_worklog = current_resource.clone();
    let fields_for_worklog = custom_fields.clone();
    let visible_for_worklog = visible_custom_ids.clone();
    let session_for_worklog = active_session;
    let query_for_worklog = current_query;
    let parent_for_worklog = current_parent;
    let type_for_worklog = current_type;
    let collapsed_for_worklog = collapsed_nodes;
    ui.on_log_work(move |key, date, time, minutes, remaining, auto_adjust| {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_action_busy() {
            return;
        }
        let Some(session) = session_for_worklog.lock().unwrap().clone() else {
            ui.set_action_status("記録前に「再同期」でJiraへ接続してください。".into());
            return;
        };
        ui.set_action_busy(true);
        ui.set_action_status("作業時間を登録中...".into());
        let weak_for_result = ui.as_weak();
        let all_for_result = all_for_worklog.clone();
        let resource_for_result = resource_for_worklog.clone();
        let fields_for_result = fields_for_worklog.clone();
        let visible_for_result = visible_for_worklog.clone();
        let query_for_result = query_for_worklog.clone();
        let parent_for_result = parent_for_worklog.clone();
        let type_for_result = type_for_worklog.clone();
        let collapsed_for_result = collapsed_for_worklog.clone();
        let key = key.to_string();
        let date = date.to_string();
        let time = time.to_string();
        let remaining = remaining.to_string();
        std::thread::spawn(move || {
            let result = jira::add_worklog(
                &session,
                &key,
                &date,
                &time,
                minutes,
                &remaining,
                auto_adjust,
            )
            .and_then(|()| jira::fetch_all_issues(&session))
            .map(|(resource, fetched, fields)| {
                let cache_error = storage::replace_issues(&resource, &fetched, &fields).err();
                (resource, fetched, fields, cache_error)
            });
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak_for_result.upgrade() else {
                    return;
                };
                ui.set_action_busy(false);
                match result {
                    Ok((resource, fetched, fields, cache_error)) => {
                        *resource_for_result.lock().unwrap() = Some(resource.clone());
                        *all_for_result.lock().unwrap() = fetched;
                        *fields_for_result.lock().unwrap() = fields;
                        let guard = all_for_result.lock().unwrap();
                        let query = query_for_result.lock().unwrap().clone();
                        let parent = parent_for_result.lock().unwrap().clone();
                        let issue_type = type_for_result.lock().unwrap().clone();
                        apply_custom_field_models(
                            &ui,
                            &fields_for_result.lock().unwrap(),
                            &visible_for_result.lock().unwrap(),
                        );
                        apply_issue_models(
                            &ui,
                            &guard,
                            &query,
                            &parent,
                            &issue_type,
                            &collapsed_for_result.lock().unwrap(),
                        );
                        if let Some(item) = guard.iter().find(|item| item.key == key) {
                            select_issue(&ui, item);
                        }
                        ui.set_action_status(
                            cache_error
                                .map(|error| format!("Jira登録済み・キャッシュ失敗: {error}"))
                                .unwrap_or_else(|| "作業時間を登録しました。".into())
                                .into(),
                        );
                    }
                    Err(error) => ui.set_action_status(format!("登録エラー: {error}").into()),
                }
            });
        });
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
        let keys: Vec<_> = filtered_rows(&items, "", "APP-102", "", &[], "", &HashSet::new())
            .into_iter()
            .map(|row| row.key.to_string())
            .collect();
        assert_eq!(keys, ["APP-102", "APP-105"]);
    }

    #[test]
    fn search_covers_description_and_comments() {
        let items = demo_issues();
        assert_eq!(
            filtered_rows(&items, "キーボード", "", "", &[], "", &HashSet::new()).len(),
            1
        );
        assert_eq!(
            filtered_rows(&items, "仮想リスト", "", "", &[], "", &HashSet::new()).len(),
            1
        );
    }

    #[test]
    fn tree_contains_nested_real_issue_models() {
        let tree = build_tree_nodes(&demo_issues(), "", &HashSet::new(), &HashSet::new());
        let nested = tree
            .iter()
            .find(|node| node.key.as_str() == "APP-105")
            .unwrap();
        assert_eq!(nested.depth, 2);
    }

    #[test]
    fn collapsed_tree_hides_descendants() {
        let collapsed = HashSet::from(["APP-102".to_owned()]);
        let tree = build_tree_nodes(&demo_issues(), "", &collapsed, &HashSet::new());
        assert!(tree.iter().any(|node| node.key.as_str() == "APP-102"));
        assert!(!tree.iter().any(|node| node.key.as_str() == "APP-105"));
    }

    #[test]
    fn issue_type_filter_keeps_tree_ancestors_as_context() {
        let items = demo_issues();
        let rows = filtered_rows(&items, "", "", "タスク", &[], "", &HashSet::new());
        assert_eq!(rows.len(), 5);
        let tree = build_tree_nodes(&items, "タスク", &HashSet::new(), &HashSet::new());
        assert!(tree.iter().any(|node| node.key.as_str() == "APP-100"));
        assert!(tree.iter().any(|node| node.key.as_str() == "APP-101"));
        assert!(!tree.iter().any(|node| node.key.as_str() == "OPS-20"));
    }

    #[test]
    fn issue_type_filter_keeps_descendants_but_respects_collapse() {
        let tree = build_tree_nodes(
            &demo_issues(),
            "タスク",
            &HashSet::from(["APP-100".to_owned(), "APP-102".to_owned()]),
            &HashSet::new(),
        );
        assert!(!tree.iter().any(|node| node.key.as_str() == "APP-105"));
    }

    #[test]
    fn issue_type_tree_includes_all_descendant_types() {
        let tree = build_tree_nodes(&demo_issues(), "エピック", &HashSet::new(), &HashSet::new());
        assert!(tree.iter().any(|node| node.key.as_str() == "APP-105"));
    }

    #[test]
    fn favorites_are_pinned_before_regular_roots() {
        let tree = build_tree_nodes(
            &demo_issues(),
            "",
            &HashSet::new(),
            &HashSet::from(["APP-105".to_owned()]),
        );
        assert_eq!(tree[0].key.as_str(), "APP-105");
        assert!(tree[0].favorite);
        assert_eq!(tree[1].key.as_str(), "APP-100");
    }

    #[test]
    fn tree_has_no_synthetic_all_issues_row() {
        let tree = build_tree_nodes(&demo_issues(), "", &HashSet::new(), &HashSet::new());
        assert!(tree.iter().all(|node| !node.key.is_empty()));
    }

    #[test]
    fn table_collapse_hides_all_descendants() {
        let rows = filtered_rows(
            &demo_issues(),
            "",
            "",
            "",
            &[],
            "",
            &HashSet::from(["APP-100".to_owned()]),
        );
        assert!(rows.iter().any(|row| row.key.as_str() == "APP-100"));
        assert!(!rows.iter().any(|row| row.key.as_str() == "APP-105"));
    }

    #[test]
    fn incomplete_status_filter_excludes_done() {
        let rows = filtered_rows(&demo_issues(), "", "", "", &[], "完了以外", &HashSet::new());
        assert!(rows.iter().all(|row| row.status.as_str() != "完了"));
    }

    #[test]
    fn table_rows_include_hierarchy_depth() {
        let rows = filtered_rows(&demo_issues(), "", "", "", &[], "", &HashSet::new());
        assert_eq!(
            rows.iter()
                .find(|row| row.key.as_str() == "APP-105")
                .unwrap()
                .depth,
            2
        );
    }

    #[test]
    fn parses_jira_style_durations() {
        assert_eq!(parse_duration("1d 2h 30m").unwrap(), 37_800);
        assert_eq!(parse_duration("0h").unwrap(), 0);
        assert!(parse_duration("90").is_err());
        assert!(parse_duration("-1h").is_err());
    }
}
