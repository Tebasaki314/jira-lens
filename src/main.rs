mod jira;
mod oauth;
mod storage;

use chrono::{Datelike, Local, NaiveDate, TimeZone};
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
    issue_type: String,
    assignee: String,
    due: String,
    estimate: String,
    spent: String,
    estimate_seconds: i64,
    spent_seconds: i64,
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
        estimate_seconds: parse_duration(estimate).unwrap_or(0),
        spent_seconds: parse_duration(spent).unwrap_or(0),
        parent: parent.into(),
        description: description.into(),
        comments: comments.into(),
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

fn to_row(issue: &Issue) -> IssueRow {
    IssueRow {
        key: issue.key.clone().into(),
        summary: issue.summary.clone().into(),
        status: issue.status.clone().into(),
        issue_type: issue.issue_type.clone().into(),
        assignee: issue.assignee.clone().into(),
        due: issue.due.clone().into(),
        estimate: issue.estimate.clone().into(),
        spent: issue.spent.clone().into(),
        parent: issue.parent.clone().into(),
    }
}

fn filtered_rows(all: &[Issue], query: &str, parent: &str, issue_type: &str) -> Vec<IssueRow> {
    let needle = query.to_lowercase();
    all.iter()
        .filter(|item| belongs_to_subtree(item, parent, all))
        .filter(|item| issue_type.is_empty() || item.issue_type == issue_type)
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

fn build_tree_nodes(
    issues: &[Issue],
    issue_type: &str,
    collapsed: &HashSet<String>,
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

    fn append(
        parent: &str,
        depth: i32,
        children: &HashMap<&str, Vec<&Issue>>,
        collapsed: &HashSet<String>,
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
                has_children: children.contains_key(item.key.as_str()),
                expanded: !collapsed.contains(&item.key),
            });
            if !collapsed.contains(&item.key) {
                append(&item.key, depth + 1, children, collapsed, visited, result);
            }
        }
    }

    let mut result = vec![TreeNode {
        key: "".into(),
        label: "すべての課題".into(),
        depth: 0,
        has_children: children.contains_key(""),
        expanded: !collapsed.contains(""),
    }];
    if !collapsed.contains("") {
        append(
            "",
            0,
            &children,
            collapsed,
            &mut HashSet::new(),
            &mut result,
        );
    }
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
    let rows = VecModel::from(filtered_rows(issues, query, parent, issue_type));
    ui.set_result_count(rows.row_count() as i32);
    ui.set_issues(ModelRc::new(rows));
    ui.set_tree_nodes(ModelRc::new(VecModel::from(build_tree_nodes(
        issues, issue_type, collapsed,
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

fn display_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if minutes == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {minutes}m")
    }
}

fn apply_burndown(ui: &AppWindow, resource: Option<&oauth::JiraResource>, root: &str) {
    let title = if root.is_empty() {
        "バーンダウン（すべて）".to_owned()
    } else {
        format!("バーンダウン（{root}）")
    };
    ui.set_burndown_title(title.into());
    let Some(resource) = resource else {
        ui.set_burndown_summary("同期後に履歴を表示します".into());
        ui.set_burndown_points(ModelRc::new(VecModel::default()));
        return;
    };
    match storage::load_burndown(&resource.id, root) {
        Ok(samples) => {
            let samples = samples
                .into_iter()
                .rev()
                .take(10)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>();
            let maximum = samples
                .iter()
                .map(|sample| sample.remaining_seconds)
                .max()
                .unwrap_or(0)
                .max(1);
            let points = samples
                .iter()
                .map(|sample| {
                    let label = Local
                        .timestamp_opt(sample.captured_at, 0)
                        .single()
                        .map(|time| time.format("%m/%d").to_string())
                        .unwrap_or_else(|| "--/--".into());
                    let height = if sample.remaining_seconds == 0 {
                        2
                    } else {
                        (sample.remaining_seconds * 78 / maximum).max(6) as i32
                    };
                    BurndownPoint {
                        label: label.into(),
                        remaining: display_duration(sample.remaining_seconds).into(),
                        height,
                    }
                })
                .collect::<Vec<_>>();
            let summary = samples
                .last()
                .map(|sample| {
                    format!(
                        "残り {}・{}回の同期履歴",
                        display_duration(sample.remaining_seconds),
                        samples.len()
                    )
                })
                .unwrap_or_else(|| "履歴データなし".into());
            ui.set_burndown_summary(summary.into());
            ui.set_burndown_points(ModelRc::new(VecModel::from(points)));
        }
        Err(error) => {
            ui.set_burndown_summary(format!("履歴エラー: {error}").into());
            ui.set_burndown_points(ModelRc::new(VecModel::default()));
        }
    }
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
    let (initial_status, initial_issues, initial_resource, has_cache) = match storage::load_latest()
    {
        Ok(Some((resource, items))) => (
            format!("キャッシュ: {}", resource.name),
            items,
            Some(resource),
            true,
        ),
        Ok(None) => ("未接続・デモ表示".into(), demo_issues(), None, false),
        Err(error) => (
            format!("キャッシュエラー: {error}"),
            demo_issues(),
            None,
            false,
        ),
    };
    let issues = Arc::new(Mutex::new(initial_issues));
    let current_resource = Arc::new(Mutex::new(initial_resource));
    let active_session = Arc::new(Mutex::new(None::<oauth::SavedSession>));
    let current_query = Arc::new(Mutex::new(String::new()));
    let current_parent = Arc::new(Mutex::new(String::new()));
    let current_type = Arc::new(Mutex::new(String::new()));
    let collapsed_nodes = Arc::new(Mutex::new(HashSet::<String>::new()));
    let calendar_state = Arc::new(Mutex::new(CalendarState::today()));
    let detail_windows = Rc::new(RefCell::new(Vec::<IssueDetailWindow>::new()));
    let chart_windows = Rc::new(RefCell::new(Vec::<BurndownChartWindow>::new()));

    ui.set_connection_status(initial_status.into());
    ui.set_has_synced_data(has_cache);
    if let Ok((assignee, due, estimate, spent)) = storage::load_column_settings() {
        ui.set_show_assignee(assignee);
        ui.set_show_due(due);
        ui.set_show_estimate(estimate);
        ui.set_show_spent(spent);
    }
    ui.set_worklog_date(
        Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string()
            .into(),
    );
    apply_issue_models(
        &ui,
        &issues.lock().unwrap(),
        "",
        "",
        "",
        &collapsed_nodes.lock().unwrap(),
    );
    apply_calendar(&ui, &calendar_state.lock().unwrap());
    apply_burndown(&ui, current_resource.lock().unwrap().as_ref(), "");

    let weak = ui.as_weak();
    let issues_for_connect = issues.clone();
    let resource_for_connect = current_resource.clone();
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
        let session_for_result = session_for_connect.clone();
        let query_for_result = query_for_connect.clone();
        let parent_for_result = parent_for_connect.clone();
        let type_for_result = type_for_connect.clone();
        let collapsed_for_result = collapsed_for_connect.clone();
        std::thread::spawn(move || {
            let result = oauth::connect().and_then(|session| {
                jira::fetch_all_issues(&session)
                    .map_err(oauth::OAuthError::message)
                    .map(|(resource, fetched)| {
                        let cache_error = storage::replace_issues(&resource, &fetched).err();
                        (session, resource, fetched, cache_error)
                    })
            });
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak_for_result.upgrade() else {
                    return;
                };
                ui.set_connection_busy(false);
                match result {
                    Ok((session, resource, fetched, cache_error)) => {
                        *session_for_result.lock().unwrap() = Some(session);
                        *resource_for_result.lock().unwrap() = Some(resource.clone());
                        *issues_for_result.lock().unwrap() = fetched;
                        query_for_result.lock().unwrap().clear();
                        parent_for_result.lock().unwrap().clear();
                        type_for_result.lock().unwrap().clear();
                        collapsed_for_result.lock().unwrap().clear();
                        let guard = issues_for_result.lock().unwrap();
                        apply_issue_models(
                            &ui,
                            &guard,
                            "",
                            "",
                            "",
                            &collapsed_for_result.lock().unwrap(),
                        );
                        apply_burndown(&ui, Some(&resource), "");
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
        window.set_description(item.description.clone().into());
        window.set_comments(item.comments.clone().into());
        if window.show().is_ok() {
            windows.borrow_mut().push(window);
        }
    });

    let weak = ui.as_weak();
    let windows = chart_windows;
    ui.on_open_burndown(move || {
        let Some(ui) = weak.upgrade() else { return };
        let Ok(window) = BurndownChartWindow::new() else {
            ui.set_action_status("バーンダウンウィンドウを作成できませんでした。".into());
            return;
        };
        window.set_chart_title(ui.get_burndown_title());
        window.set_summary(ui.get_burndown_summary());
        window.set_points(ui.get_burndown_points());
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
    let resource_for_filter = current_resource.clone();
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
            apply_burndown(
                &ui,
                resource_for_filter.lock().unwrap().as_ref(),
                parent.as_str(),
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
    ui.on_columns_changed(move |assignee, due, estimate, spent| {
        if let Err(error) = storage::save_column_settings((assignee, due, estimate, spent))
            && let Some(ui) = weak.upgrade()
        {
            ui.set_action_status(error.into());
        }
    });

    let weak = ui.as_weak();
    let all_for_save = issues.clone();
    let resource_for_save = current_resource.clone();
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
            .map(|(resource, fetched)| {
                let cache_error = storage::replace_issues(&resource, &fetched).err();
                (resource, fetched, cache_error)
            });
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak_for_result.upgrade() else {
                    return;
                };
                ui.set_action_busy(false);
                match result {
                    Ok((resource, fetched, cache_error)) => {
                        *resource_for_result.lock().unwrap() = Some(resource.clone());
                        *all_for_result.lock().unwrap() = fetched;
                        let guard = all_for_result.lock().unwrap();
                        let query = query_for_result.lock().unwrap().clone();
                        let parent = parent_for_result.lock().unwrap().clone();
                        let issue_type = type_for_result.lock().unwrap().clone();
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
                        apply_burndown(&ui, Some(&resource), &parent);
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
    let session_for_worklog = active_session;
    let query_for_worklog = current_query;
    let parent_for_worklog = current_parent;
    let type_for_worklog = current_type;
    let collapsed_for_worklog = collapsed_nodes;
    ui.on_log_work(move |key, date, time, minutes| {
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
        let query_for_result = query_for_worklog.clone();
        let parent_for_result = parent_for_worklog.clone();
        let type_for_result = type_for_worklog.clone();
        let collapsed_for_result = collapsed_for_worklog.clone();
        let key = key.to_string();
        let date = date.to_string();
        let time = time.to_string();
        std::thread::spawn(move || {
            let result = jira::add_worklog(&session, &key, &date, &time, minutes)
                .and_then(|()| jira::fetch_all_issues(&session))
                .map(|(resource, fetched)| {
                    let cache_error = storage::replace_issues(&resource, &fetched).err();
                    (resource, fetched, cache_error)
                });
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak_for_result.upgrade() else {
                    return;
                };
                ui.set_action_busy(false);
                match result {
                    Ok((resource, fetched, cache_error)) => {
                        *resource_for_result.lock().unwrap() = Some(resource.clone());
                        *all_for_result.lock().unwrap() = fetched;
                        let guard = all_for_result.lock().unwrap();
                        let query = query_for_result.lock().unwrap().clone();
                        let parent = parent_for_result.lock().unwrap().clone();
                        let issue_type = type_for_result.lock().unwrap().clone();
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
                        apply_burndown(&ui, Some(&resource), &parent);
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
        let keys: Vec<_> = filtered_rows(&items, "", "APP-102", "")
            .into_iter()
            .map(|row| row.key.to_string())
            .collect();
        assert_eq!(keys, ["APP-102", "APP-105"]);
    }

    #[test]
    fn search_covers_description_and_comments() {
        let items = demo_issues();
        assert_eq!(filtered_rows(&items, "キーボード", "", "").len(), 1);
        assert_eq!(filtered_rows(&items, "仮想リスト", "", "").len(), 1);
    }

    #[test]
    fn tree_contains_nested_real_issue_models() {
        let tree = build_tree_nodes(&demo_issues(), "", &HashSet::new());
        let nested = tree
            .iter()
            .find(|node| node.key.as_str() == "APP-105")
            .unwrap();
        assert_eq!(nested.depth, 2);
    }

    #[test]
    fn collapsed_tree_hides_descendants() {
        let collapsed = HashSet::from(["APP-102".to_owned()]);
        let tree = build_tree_nodes(&demo_issues(), "", &collapsed);
        assert!(tree.iter().any(|node| node.key.as_str() == "APP-102"));
        assert!(!tree.iter().any(|node| node.key.as_str() == "APP-105"));
    }

    #[test]
    fn issue_type_filter_keeps_tree_ancestors_as_context() {
        let items = demo_issues();
        let rows = filtered_rows(&items, "", "", "タスク");
        assert_eq!(rows.len(), 5);
        let tree = build_tree_nodes(&items, "タスク", &HashSet::new());
        assert!(tree.iter().any(|node| node.key.as_str() == "APP-100"));
        assert!(tree.iter().any(|node| node.key.as_str() == "APP-101"));
        assert!(!tree.iter().any(|node| node.key.as_str() == "OPS-20"));
    }

    #[test]
    fn parses_jira_style_durations() {
        assert_eq!(parse_duration("1d 2h 30m").unwrap(), 37_800);
        assert_eq!(parse_duration("0h").unwrap(), 0);
        assert!(parse_duration("90").is_err());
        assert!(parse_duration("-1h").is_err());
    }
}
