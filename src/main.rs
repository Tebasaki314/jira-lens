mod oauth;

use slint::{Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

slint::include_modules!();

#[derive(Clone)]
struct DemoIssue {
    key: &'static str,
    summary: &'static str,
    status: &'static str,
    assignee: &'static str,
    due: &'static str,
    estimate: &'static str,
    spent: &'static str,
    parent: &'static str,
    description: &'static str,
    comments: &'static str,
}

fn demo_issues() -> Vec<DemoIssue> {
    vec![
        DemoIssue {
            key: "APP-100",
            summary: "デスクトップ版 v1",
            status: "進行中",
            assignee: "Hiroshi",
            due: "2026-09-12",
            estimate: "40h",
            spent: "12h",
            parent: "",
            description: "Jiraを軽快に閲覧・更新するデスクトップアプリ",
            comments: "MVPの対象範囲を確定",
        },
        DemoIssue {
            key: "APP-101",
            summary: "親子ツリー表示",
            status: "完了",
            assignee: "Hiroshi",
            due: "2026-08-24",
            estimate: "8h",
            spent: "7h 30m",
            parent: "APP-100",
            description: "Epic、親タスク、サブタスクを一つのツリーで表示",
            comments: "仮想リストを使用する",
        },
        DemoIssue {
            key: "APP-102",
            summary: "課題一覧テーブル",
            status: "進行中",
            assignee: "Mika",
            due: "2026-08-28",
            estimate: "12h",
            spent: "4h",
            parent: "APP-100",
            description: "Excelのように見通しのよい一覧を作る",
            comments: "列の表示切替を追加",
        },
        DemoIssue {
            key: "APP-105",
            summary: "日付セルの編集",
            status: "未着手",
            assignee: "Mika",
            due: "2026-08-27",
            estimate: "3h",
            spent: "0h",
            parent: "APP-102",
            description: "カレンダーから期限を入力できるようにする",
            comments: "キーボード操作も後で対応",
        },
        DemoIssue {
            key: "APP-103",
            summary: "時間記録フォーム",
            status: "レビュー",
            assignee: "Sora",
            due: "2026-08-30",
            estimate: "6h",
            spent: "5h",
            parent: "APP-100",
            description: "開始日時と作業時間を少ない操作で登録",
            comments: "15分単位の候補が便利",
        },
        DemoIssue {
            key: "APP-104",
            summary: "バーンダウン",
            status: "未着手",
            assignee: "Sora",
            due: "2026-09-05",
            estimate: "8h",
            spent: "0h",
            parent: "APP-100",
            description: "任意の親課題以下を対象に残時間を可視化",
            comments: "見積変更も履歴に反映する",
        },
        DemoIssue {
            key: "OPS-20",
            summary: "リリース準備",
            status: "未着手",
            assignee: "Hiroshi",
            due: "2026-09-10",
            estimate: "10h",
            spent: "0h",
            parent: "",
            description: "macOSとWindows向けに署名済み成果物を作成",
            comments: "CI構築が必要",
        },
    ]
}

fn to_row(issue: &DemoIssue) -> IssueRow {
    IssueRow {
        key: issue.key.into(),
        summary: issue.summary.into(),
        status: issue.status.into(),
        assignee: issue.assignee.into(),
        due: issue.due.into(),
        estimate: issue.estimate.into(),
        spent: issue.spent.into(),
        parent: issue.parent.into(),
    }
}

fn filtered_rows(all: &[DemoIssue], query: &str, parent: &str) -> Vec<IssueRow> {
    let needle = query.to_lowercase();
    all.iter()
        .filter(|i| belongs_to_subtree(i, parent, all))
        .filter(|i| {
            needle.is_empty()
                || [i.key, i.summary, i.description, i.comments]
                    .iter()
                    .any(|v| v.to_lowercase().contains(&needle))
        })
        .map(to_row)
        .collect()
}

fn belongs_to_subtree(issue: &DemoIssue, root: &str, all: &[DemoIssue]) -> bool {
    if root.is_empty() {
        return true;
    }
    let mut key = issue.key;
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
        key = current.parent;
    }
    false
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let issues = Rc::new(demo_issues());
    let rows = Rc::new(VecModel::from(filtered_rows(&issues, "", "")));
    ui.set_issues(ModelRc::from(rows.clone()));

    let weak = ui.as_weak();
    ui.on_connect_oauth(move || {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_connection_busy() {
            return;
        }
        ui.set_connection_busy(true);
        ui.set_connection_status("ブラウザでAtlassianの認可を完了してください".into());
        let weak_for_result = ui.as_weak();
        std::thread::spawn(move || {
            let result = oauth::connect();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak_for_result.upgrade() else {
                    return;
                };
                ui.set_connection_busy(false);
                match result {
                    Ok(session) => {
                        let site = &session.resources[0];
                        ui.set_connection_status(format!("接続済み: {}", site.name).into());
                    }
                    Err(error) => {
                        ui.set_connection_status(format!("接続エラー: {error}").into());
                    }
                }
            });
        });
    });

    let weak = ui.as_weak();
    let issues_for_select = issues.clone();
    ui.on_select_issue(move |key| {
        let Some(ui) = weak.upgrade() else { return };
        if let Some(issue) = issues_for_select.iter().find(|i| i.key == key.as_str()) {
            ui.set_selected_key(issue.key.into());
            ui.set_selected_summary(issue.summary.into());
            ui.set_selected_description(issue.description.into());
            ui.set_selected_comments(issue.comments.into());
            ui.set_selected_due(issue.due.into());
            ui.set_selected_estimate(issue.estimate.into());
            ui.set_selected_spent(issue.spent.into());
        }
    });

    let current_query = Rc::new(std::cell::RefCell::new(String::new()));
    let current_parent = Rc::new(std::cell::RefCell::new(String::new()));

    let weak = ui.as_weak();
    let all = issues.clone();
    let model = rows.clone();
    let query_state = current_query.clone();
    let parent_state = current_parent.clone();
    ui.on_search(move |query| {
        *query_state.borrow_mut() = query.to_string();
        model.set_vec(filtered_rows(
            &all,
            &query_state.borrow(),
            &parent_state.borrow(),
        ));
        if let Some(ui) = weak.upgrade() {
            ui.set_result_count(model.row_count() as i32);
        }
    });

    let weak = ui.as_weak();
    let all = issues.clone();
    let model = rows.clone();
    let query_state = current_query.clone();
    let parent_state = current_parent.clone();
    ui.on_filter_parent(move |parent| {
        *parent_state.borrow_mut() = parent.to_string();
        model.set_vec(filtered_rows(
            &all,
            &query_state.borrow(),
            &parent_state.borrow(),
        ));
        if let Some(ui) = weak.upgrade() {
            ui.set_active_parent(if parent.is_empty() {
                SharedString::from("すべての課題")
            } else {
                parent
            });
            ui.set_result_count(model.row_count() as i32);
        }
    });

    ui.set_result_count(rows.row_count() as i32);
    ui.invoke_select_issue("APP-102".into());
    ui.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_filter_includes_all_descendants() {
        let issues = demo_issues();
        let keys: Vec<_> = filtered_rows(&issues, "", "APP-102")
            .into_iter()
            .map(|row| row.key.to_string())
            .collect();
        assert_eq!(keys, ["APP-102", "APP-105"]);
    }

    #[test]
    fn search_covers_description_and_comments() {
        let issues = demo_issues();
        assert_eq!(filtered_rows(&issues, "キーボード", "").len(), 1);
        assert_eq!(filtered_rows(&issues, "仮想リスト", "").len(), 1);
    }
}
