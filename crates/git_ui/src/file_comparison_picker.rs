use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, ParentElement,
    Render, SharedString, Styled, WeakEntity, Window, rems,
};
use picker::{Picker, PickerDelegate};
use project::Project;
use std::sync::Arc;
use ui::{ListItem, ListItemSpacing, prelude::*};
use workspace::{ModalView, Workspace};

pub struct FileComparisonPicker {
    picker: Entity<Picker<FileComparisonDelegate>>,
}

impl ModalView for FileComparisonPicker {}

impl FileComparisonPicker {
    pub fn new(
        active_buffer: Entity<language::Buffer>,
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        open_buffers: Vec<(Entity<language::Buffer>, SharedString)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let delegate = FileComparisonDelegate::new(active_buffer, workspace, project, open_buffers);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx));

        Self { picker }
    }
}

impl EventEmitter<DismissEvent> for FileComparisonPicker {}

impl Focusable for FileComparisonPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for FileComparisonPicker {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("FileComparisonPicker")
            .w(rems(34.))
            .child(self.picker.clone())
    }
}

#[derive(Clone)]
struct FileMatch {
    buffer: Entity<language::Buffer>,
    display_text: SharedString,
}

pub struct FileComparisonDelegate {
    active_buffer: Entity<language::Buffer>,
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    all_matches: Vec<FileMatch>,
    matches: Vec<FileMatch>,
    selected_index: usize,
}

impl FileComparisonDelegate {
    fn new(
        active_buffer: Entity<language::Buffer>,
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        open_buffers: Vec<(Entity<language::Buffer>, SharedString)>,
    ) -> Self {
        let all_matches: Vec<FileMatch> = open_buffers
            .into_iter()
            .map(|(buffer, display_text)| FileMatch {
                buffer,
                display_text,
            })
            .collect();

        let matches = all_matches.clone();

        Self {
            active_buffer,
            workspace,
            project,
            all_matches,
            matches,
            selected_index: 0,
        }
    }
}

impl PickerDelegate for FileComparisonDelegate {
    type ListItem = ListItem;

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Select a file to compare with...".into()
    }

    fn no_matches_text(&self, _window: &mut Window, _cx: &mut App) -> Option<SharedString> {
        Some("No other files open".into())
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix.min(self.matches.len().saturating_sub(1));
        cx.notify();
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> gpui::Task<()> {
        if query.is_empty() {
            // Reset to show all matches
            self.matches = self.all_matches.clone();
            self.selected_index = 0;
            cx.notify();
            return gpui::Task::ready(());
        }

        // Filter matches by query
        let query_lower = query.to_lowercase();
        self.matches = self
            .all_matches
            .iter()
            .filter(|tab| tab.display_text.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();

        self.selected_index = 0;
        cx.notify();
        gpui::Task::ready(())
    }

    fn confirm(&mut self, _secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if self.matches.is_empty() {
            return;
        }

        let selected_match = &self.matches[self.selected_index];
        let active_buffer = self.active_buffer.clone();
        let compare_buffer = selected_match.buffer.clone();
        let project = self.project.clone();

        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        // Get the titles
        let active_title: SharedString = active_buffer
            .read(cx)
            .file()
            .map(|f| f.file_name(cx).to_string())
            .unwrap_or_else(|| "Untitled".to_string())
            .into();

        let compare_title = selected_match.display_text.clone();

        let languages = project.read(cx).languages().clone();

        window
            .spawn(cx, async move |mut cx| {
                let diff =
                    super::build_diff_buffer(&active_buffer, &compare_buffer, languages, &mut cx)
                        .await?;

                workspace.update_in(cx, |workspace, window, cx| {
                    let diff_view = cx.new(|cx| {
                        super::TextDiffView::new_from_buffers(
                            compare_buffer,
                            active_buffer,
                            diff,
                            project.clone(),
                            compare_title,
                            active_title,
                            window,
                            cx,
                        )
                    });

                    let pane = workspace.active_pane();
                    pane.update(cx, |pane, cx| {
                        pane.add_item(Box::new(diff_view.clone()), true, true, None, window, cx);
                    });

                    anyhow::Ok(())
                })
            })
            .detach_and_log_err(cx);

        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _window: &mut Window, _cx: &mut Context<Picker<Self>>) {}

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let file_match = &self.matches[ix];

        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .child(Label::new(file_match.display_text.clone())),
        )
    }
}
