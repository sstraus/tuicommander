use super::{STORY_LIST_QUERY_INVALID, StoryListFilter, StoryListPage, StoryStore};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};

const CURSOR_PREFIX: &str = "stories-v1.";
const MAX_CURSOR_BYTES: usize = 1024;

fn invalid(message: &str) -> String {
    format!("{STORY_LIST_QUERY_INVALID}{message}")
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ListCursor {
    plan_id: String,
    filter: StoryListFilter,
    after_id: String,
}

impl StoryStore {
    /// Presentation query only: dispatch and dependency readers keep using the complete plan.
    pub fn list_stories_filtered(
        &self,
        plan_id: &str,
        filter: &StoryListFilter,
        limit: Option<u16>,
        cursor: Option<&str>,
    ) -> Result<StoryListPage, String> {
        if filter
            .priority
            .is_some_and(|priority| !(1..=3).contains(&priority))
        {
            return Err(invalid("story list priority must be between 1 and 3"));
        }
        if limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
            return Err(invalid("story list limit must be between 1 and 100"));
        }
        let after_id = if let Some(cursor) = cursor {
            if limit.is_none() {
                return Err(invalid("story list cursor requires limit"));
            }
            if cursor.len() > MAX_CURSOR_BYTES {
                return Err(invalid("story list cursor exceeds 1024 bytes"));
            }
            let bytes = cursor
                .strip_prefix(CURSOR_PREFIX)
                .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
                .ok_or_else(|| invalid("invalid story list cursor"))?;
            let cursor: ListCursor =
                serde_json::from_slice(&bytes).map_err(|_| invalid("invalid story list cursor"))?;
            if cursor.plan_id != plan_id || cursor.filter != *filter {
                return Err(invalid("story list cursor does not match plan and filters"));
            }
            let id = uuid::Uuid::parse_str(&cursor.after_id)
                .map_err(|_| invalid("invalid story list cursor id"))?;
            if id.to_string() != cursor.after_id {
                return Err(invalid("invalid story list cursor id"));
            }
            Some(cursor.after_id)
        } else {
            None
        };
        // Preserve the existing id order and full internal read; only the response is projected.
        let mut stories = self.list_stories(plan_id)?;
        stories.retain(|story| {
            story.archived == filter.archived
                && filter.state.is_none_or(|state| story.status == state)
                && filter
                    .priority
                    .is_none_or(|priority| story.priority == priority)
                && filter
                    .story_type
                    .is_none_or(|kind| story.story_type == Some(kind))
                && after_id.as_ref().is_none_or(|id| story.id > *id)
        });
        let mut next_cursor = None;
        if let Some(limit) = limit {
            let limit = usize::from(limit);
            if stories.len() > limit {
                stories.truncate(limit);
                let cursor = ListCursor {
                    plan_id: plan_id.to_string(),
                    filter: filter.clone(),
                    after_id: stories.last().ok_or("story list page is empty")?.id.clone(),
                };
                let encoded = serde_json::to_vec(&cursor)
                    .map_err(|error| format!("encode story list cursor: {error}"))?;
                next_cursor = Some(format!(
                    "{CURSOR_PREFIX}{}",
                    URL_SAFE_NO_PAD.encode(encoded)
                ));
            }
        }
        Ok(StoryListPage {
            value: stories,
            next_cursor,
        })
    }
}
