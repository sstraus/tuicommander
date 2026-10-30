/** Synthetic identities exercise production layout; not a captured agent session. */
import { render } from "solid-js/web";
import { ActivityItem } from "../../components/ActivityItem";
import { SessionHeaderOverlay } from "../../components/SessionHeaderOverlay";
import { FilesScreen } from "../../screens/FilesScreen";
import { SessionsScreen } from "../../screens/SessionsScreen";
import "../../mobile.css";

const long = "gate-auto-" + "a".repeat(60);
const mode = new URLSearchParams(location.search).get("mode");
render(
	() =>
		mode === "files" ? (
			<FilesScreen initialRepo={{ worktreePath: "/layout/" + long, cwd: "/layout/" + long }} />
		) : mode === "activity" ? (
			<ActivityItem
				item={{
					id: "layout",
					title: long,
					subtitle: long,
					icon: "",
					createdAt: Date.now(),
					pluginId: "layout",
					sectionId: "layout",
					dismissible: false,
				}}
				onTap={() => {}}
			/>
		) : mode === "progress" ? (
			<SessionHeaderOverlay
				mode="progress"
				session={{ session_id: "layout", cwd: "/layout", worktree_path: "/layout", worktree_branch: null }}
				onClose={() => {}}
			/>
		) : (
			<SessionsScreen
				sessions={[
					{
						session_id: "long-name-layout",
						display_name: long,
						cwd: "/layout/" + long,
						worktree_path: null,
						worktree_branch: long,
						state: {
							agent_type: "ego",
							awaiting_input: false,
							rate_limited: false,
							last_activity_ms: Date.now() - 120000,
							agent_intent: long,
							current_task: long,
						},
					},
				]}
				loading={false}
				refreshing={false}
				error={null}
				onRefresh={() => {}}
				onSelectSession={() => {}}
			/>
		),
	document.getElementById("mobile-app")!,
);
