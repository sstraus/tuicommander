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

/// Next-action group of a status: 0 QA, 1 in progress, 2 ready, 3 done.
/// The one place that maps provider statuses to the panel order.
fn order_group(status: StoryStatus) -> u8 {
    match status {
        StoryStatus::Qa | StoryStatus::Review => 0,
        StoryStatus::InProgress | StoryStatus::Blocked => 1,
        StoryStatus::Ready | StoryStatus::Backlog => 2,
        StoryStatus::Done | StoryStatus::WontFix => 3,
    }
}

pub(crate) fn project(
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
    let mut ordered: Vec<&StoryRead> = stories.iter().collect();
    // Stable sort: equal group and priority keep the current order.
    ordered.sort_by_key(|read| (order_group(read.story.status), read.story.priority));
    for read in ordered {
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

pub(crate) fn validate(filter: &StoryFilter) -> Result<(), String> {
    if filter.untyped_only && filter.story_type.is_some() {
        return Err("untypedOnly and storyType are mutually exclusive".into());
    }
    Ok(())
}

impl StoryStore {
    /// Keep aggregation and column membership identical for every transport and client.
    pub fn project_story_view(
        &self,
        project_path: &str,
        archived_plans: bool,
        filter: &StoryFilter,
    ) -> Result<super::ProjectStoryView, String> {
        validate(filter)?;
        let mut stories = Vec::new();
        for plan in self.list_plans_archived(project_path, archived_plans)? {
            stories.extend(self.plan_view_archived(&plan.id, filter.archived)?.stories);
        }
        let (visible_story_ids, columns) = project(&stories, filter);
        Ok(super::ProjectStoryView {
            stories,
            visible_story_ids,
            columns,
        })
    }

    /// Filter only the presentation projection. Plan completion and dependency detail use all records.
    pub fn filtered_plan_view(
        &self,
        plan_id: &str,
        filter: &StoryFilter,
    ) -> Result<PlanView, String> {
        validate(filter)?;
        let mut view = self.plan_view_archived(plan_id, filter.archived)?;
        (view.visible_story_ids, view.columns) = project(&view.stories, filter);
        Ok(view)
    }
}
