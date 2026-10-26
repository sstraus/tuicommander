use super::{
    PlanView, StoryColumn, StoryFilter, StoryRead, StoryStatus, StoryStore, StoryVisibility,
};

const BOARD_STATUSES: &[StoryStatus] = &[
    StoryStatus::Backlog,
    StoryStatus::Ready,
    StoryStatus::InProgress,
    StoryStatus::Review,
    StoryStatus::Qa,
    StoryStatus::Blocked,
    StoryStatus::WontFix,
    StoryStatus::Done,
];

pub(super) fn project(
    stories: &[StoryRead],
    filter: &StoryFilter,
) -> (Vec<String>, Vec<StoryColumn>) {
    let mut columns: Vec<_> = BOARD_STATUSES
        .iter()
        .map(|status| StoryColumn {
            status: *status,
            story_ids: Vec::new(),
        })
        .collect();
    let mut ids = Vec::new();
    for read in stories {
        let story = &read.story;
        let closed = matches!(story.status, StoryStatus::Done | StoryStatus::WontFix);
        if story.archived != filter.archived
            || matches!(filter.visibility, StoryVisibility::Open) && closed
            || matches!(filter.visibility, StoryVisibility::Closed) && !closed
            || matches!(filter.visibility, StoryVisibility::Qa) && story.status != StoryStatus::Qa
            || filter
                .story_type
                .is_some_and(|kind| story.story_type != Some(kind))
            || filter.untyped_only && story.story_type.is_some()
        {
            continue;
        }
        ids.push(story.id.clone());
        if let Some(column) = columns
            .iter_mut()
            .find(|column| column.status == story.status)
        {
            column.story_ids.push(story.id.clone());
        }
    }
    (ids, columns)
}

impl StoryStore {
    /// Filter only the presentation projection. Plan completion and dependency detail use all records.
    pub fn filtered_plan_view(
        &self,
        plan_id: &str,
        filter: &StoryFilter,
    ) -> Result<PlanView, String> {
        if filter.untyped_only && filter.story_type.is_some() {
            return Err("untypedOnly and storyType are mutually exclusive".into());
        }
        let mut view = self.plan_view_archived(plan_id, filter.archived)?;
        (view.visible_story_ids, view.columns) = project(&view.stories, filter);
        Ok(view)
    }
}
