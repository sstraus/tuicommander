import { type Accessor, lazy, type Setter, Show, Suspense } from "solid-js";
import releaseNotes from "../../assets/release-notes.json";
import type { useConfirmDialog } from "../../hooks/useConfirmDialog";
import type { FolderDropRequest } from "../../hooks/useFileDrop";
import type { useGitOperations } from "../../hooks/useGitOperations";
import { invoke } from "../../invoke";
import { remoteConnectionsStore } from "../../stores/remoteConnections";
import { repositoriesStore } from "../../stores/repositories";
import { terminalsStore } from "../../stores/terminals";
import { ConfirmDialog } from "../ConfirmDialog";
import { ContextMenu, type createContextMenu } from "../ContextMenu";
import { CreateBranchDialog } from "../CreateBranchDialog";
import { CreateWorktreeDialog } from "../CreateWorktreeDialog";
import { GeneratorsModal } from "../GeneratorsModal";
import { McpConfirmHost } from "../McpConfirmHost/McpConfirmHost";
import {
	type CleanupStep,
	PostMergeCleanupDialog,
	type StepId,
	type StepStatus,
} from "../PostMergeCleanupDialog/PostMergeCleanupDialog";
import { ProcessManagerModal } from "../ProcessManagerModal/ProcessManagerModal";
import { PromptDialog } from "../PromptDialog";
import { CloseDialog } from "../QuitDialog/CloseDialog";
import { RemoteQrDialog } from "../RemoteQrDialog";
import { RemoteRepoPicker } from "../RemoteRepoPicker";
import { RenameBranchDialog } from "../RenameBranchDialog";
import { RunCommandDialog } from "../RunCommandDialog";
import type { SettingsContext } from "../SettingsPanel";
import { TaskQueuePanel } from "../TaskQueuePanel";
import { UpdateProgressDialog } from "../UpdateProgressDialog";
import { WhatsNewDialog } from "../WhatsNewDialog/WhatsNewDialog";
import { WorkflowApprovalHost } from "../WorkflowDesigner/WorkflowApprovalHost";

const SettingsPanel = lazy(() => import("../SettingsPanel").then((module) => ({ default: module.SettingsPanel })));
const HelpPanel = lazy(() => import("../HelpPanel").then((module) => ({ default: module.HelpPanel })));

type GitOperations = ReturnType<typeof useGitOperations>;
type ConfirmDialogs = ReturnType<typeof useConfirmDialog>;

interface PanelOverlaysContract {
	settingsVisible: Accessor<boolean>;
	closeSettings: () => void;
	settingsInitialTab: Accessor<string | undefined>;
	settingsInitialSection: Accessor<string | undefined>;
	settingsContext: Accessor<SettingsContext>;
	taskQueueVisible: Accessor<boolean>;
	closeTaskQueue: () => void;
	helpVisible: Accessor<boolean>;
	closeHelp: () => void;
}

interface GitOverlaysContract {
	renameVisible: Accessor<boolean>;
	closeRename: () => void;
	branchToRename: GitOperations["branchToRename"];
	onRename: GitOperations["handleRenameBranch"];
	createVisible: Accessor<boolean>;
	closeCreate: () => void;
	branchToCreate: GitOperations["branchToCreate"];
	onCreate: GitOperations["handleCreateBranch"];
	worktreeState: GitOperations["worktreeDialogState"];
	closeWorktree: () => void;
	onGenerateWorktreeName: GitOperations["generateWorktreeName"];
	onCreateWorktree: GitOperations["confirmCreateWorktree"];
	runVisible: Accessor<boolean>;
	closeRun: () => void;
	activeRunCommand: GitOperations["activeRunCommand"];
	onRun: GitOperations["executeRunCommand"];
}

interface PromptOverlaysContract {
	terminalRenameVisible: Accessor<boolean>;
	closeTerminalRename: () => void;
	terminalRenameDefault: Accessor<string>;
	openPathVisible: Accessor<boolean>;
	resolveOpenPath: (value: string | null) => void;
	repoPathVisible: Accessor<boolean>;
	resolveRepoPath: (value: string | null) => void;
	/** The machine the remote repo picker is browsing, or null when it is closed. */
	remotePickerConnectionId: Accessor<string | null>;
	resolveRemotePicker: (value: string | null) => void;
}

interface ConfirmationOverlaysContract {
	dialogState: ConfirmDialogs["dialogState"];
	onClose: ConfirmDialogs["handleClose"];
	onConfirm: ConfirmDialogs["handleConfirm"];
	onDiscard: ConfirmDialogs["handleDiscard"];
	pendingFolderDrop: Accessor<FolderDropRequest | null>;
	setPendingFolderDrop: Setter<FolderDropRequest | null>;
}

interface UtilityOverlaysContract {
	processManagerVisible: Accessor<boolean>;
	closeProcessManager: () => void;
	generatorsVisible: Accessor<boolean>;
	closeGenerators: () => void;
	remoteQrVisible: Accessor<boolean>;
	closeRemoteQr: () => void;
	whatsNewVersion: Accessor<string | null>;
	setWhatsNewVersion: Setter<string | null>;
}

interface CleanupOverlayContract {
	context: GitOperations["mergePendingCtx"];
	action: Accessor<"archive" | "delete">;
	setAction: Setter<"archive" | "delete">;
	executing: Accessor<boolean>;
	stepStatuses: Accessor<Partial<Record<StepId, StepStatus>>>;
	stepErrors: Accessor<Partial<Record<StepId, string>>>;
	stepNotes: Accessor<Partial<Record<StepId, string>>>;
	onExecute: (steps: CleanupStep[], options?: { unstash?: boolean }) => Promise<void>;
	onSkip: () => void;
}

interface ApplicationOverlaysProps {
	panels: PanelOverlaysContract;
	contextMenu: ReturnType<typeof createContextMenu>;
	getContextMenuItems: () => Parameters<typeof ContextMenu>[0]["items"];
	git: GitOverlaysContract;
	prompts: PromptOverlaysContract;
	confirmations: ConfirmationOverlaysContract;
	utilities: UtilityOverlaysContract;
	cleanup: CleanupOverlayContract;
}

export function folderDropMessage(request: FolderDropRequest | null): string {
	if (!request) return "";
	const verb = request.mode === "copy" ? "copy" : "move";
	const items = request.paths.length === 1 ? "1 item" : `${request.paths.length} items`;
	return `About to recursively ${verb} ${items} into ${request.destDir}. Existing files with the same name will be skipped.`;
}

function GitDialogOverlays(props: { contract: GitOverlaysContract }) {
	const git = props.contract;
	return (
		<>
			<RenameBranchDialog
				visible={git.renameVisible()}
				currentName={git.branchToRename()?.branchName || ""}
				onClose={git.closeRename}
				onRename={git.onRename}
			/>
			<CreateBranchDialog
				visible={git.createVisible()}
				startPoint={git.branchToCreate()?.startPoint}
				onClose={git.closeCreate}
				onCreate={git.onCreate}
			/>
			<CreateWorktreeDialog
				visible={git.worktreeState() !== null}
				suggestedName={git.worktreeState()?.suggestedName ?? ""}
				existingBranches={git.worktreeState()?.existingBranches ?? []}
				worktreeBranches={git.worktreeState()?.worktreeBranches ?? []}
				worktreesDir={git.worktreeState()?.worktreesDir ?? ""}
				baseRefs={git.worktreeState()?.baseRefs}
				onGenerateName={git.onGenerateWorktreeName}
				onClose={git.closeWorktree}
				onCreate={git.onCreateWorktree}
			/>
			<RunCommandDialog
				visible={git.runVisible()}
				savedCommand={git.activeRunCommand() || ""}
				onClose={git.closeRun}
				onSaveAndRun={(command) => {
					git.closeRun();
					git.onRun(command);
				}}
			/>
		</>
	);
}

function ConfirmationOverlays(props: { contract: ConfirmationOverlaysContract }) {
	const confirmations = props.contract;
	return (
		<>
			<ConfirmDialog
				visible={confirmations.dialogState() !== null}
				title={confirmations.dialogState()?.title ?? ""}
				message={confirmations.dialogState()?.message ?? ""}
				confirmLabel={confirmations.dialogState()?.confirmLabel}
				cancelLabel={confirmations.dialogState()?.cancelLabel}
				discardLabel={confirmations.dialogState()?.discardLabel}
				discardDanger={confirmations.dialogState()?.discardDanger}
				discardDisabled={confirmations.dialogState()?.discardDisabled}
				archiveChecklist={confirmations.dialogState()?.archiveChecklist}
				workspaceName={confirmations.dialogState()?.workspaceName}
				kind={confirmations.dialogState()?.kind}
				defaultButton={confirmations.dialogState()?.defaultButton}
				autoCancelMs={confirmations.dialogState()?.autoCancelMs}
				autoConfirmMs={confirmations.dialogState()?.autoConfirmMs}
				onClose={confirmations.onClose}
				onConfirm={confirmations.onConfirm}
				onDiscard={confirmations.onDiscard}
			/>
			<ConfirmDialog
				visible={confirmations.pendingFolderDrop() !== null}
				title={confirmations.pendingFolderDrop()?.mode === "copy" ? "Copy folder(s)?" : "Move folder(s)?"}
				message={folderDropMessage(confirmations.pendingFolderDrop())}
				confirmLabel={confirmations.pendingFolderDrop()?.mode === "copy" ? "Copy" : "Move"}
				onClose={() => confirmations.setPendingFolderDrop(null)}
				onConfirm={async () => {
					const request = confirmations.pendingFolderDrop();
					confirmations.setPendingFolderDrop(null);
					if (!request) return;
					const { confirmFolderDrop } = await import("../../hooks/useFileDrop");
					await confirmFolderDrop(request);
				}}
			/>
		</>
	);
}

function CleanupOverlay(props: { contract: CleanupOverlayContract }) {
	return (
		<Show when={props.contract.context() !== null}>
			{(() => {
				const context = props.contract.context()!;
				const repo = repositoriesStore.get(context.repoPath);
				const branch = repo?.workspaces[context.workspaceId];
				return (
					<PostMergeCleanupDialog
						branchName={context.branchName}
						baseBranch={context.baseBranch}
						repoPath={context.repoPath}
						isOnBaseBranch={(repo?.activeWorkspaceId ?? "") === context.baseBranch}
						isDefaultBranch={branch?.isMain ?? false}
						hasTerminals={(branch?.terminals.length ?? 0) > 0}
						hasDirtyFiles={context.hasDirtyFiles}
						worktreeDirty={context.worktreeDirty}
						removalWarnings={context.removalWarnings}
						submoduleUnpushedCommits={context.submoduleUnpushedCommits}
						worktreeAction={props.contract.action()}
						onWorktreeActionChange={props.contract.setAction}
						executing={props.contract.executing()}
						stepStatuses={props.contract.stepStatuses()}
						stepErrors={props.contract.stepErrors()}
						stepNotes={props.contract.stepNotes()}
						onExecute={props.contract.onExecute}
						onSkip={props.contract.onSkip}
					/>
				);
			})()}
		</Show>
	);
}

/** Renders application dialogs and owns overlay-only keyboard behavior. */
export function ApplicationOverlays(props: ApplicationOverlaysProps) {
	const whatsNewEntry = () => {
		const version = props.utilities.whatsNewVersion();
		if (!version) return null;
		return (
			(releaseNotes as Record<string, { highlights: string[]; contributions?: { text: string; author: string }[] }>)[
				version
			] ?? null
		);
	};

	return (
		<>
			<Suspense>
				<SettingsPanel
					visible={props.panels.settingsVisible()}
					onClose={props.panels.closeSettings}
					initialTab={props.panels.settingsInitialTab()}
					initialSection={props.panels.settingsInitialSection()}
					context={props.panels.settingsContext()}
				/>
			</Suspense>
			<TaskQueuePanel visible={props.panels.taskQueueVisible()} onClose={props.panels.closeTaskQueue} />
			<ContextMenu
				items={props.getContextMenuItems()}
				x={props.contextMenu.position().x}
				y={props.contextMenu.position().y}
				visible={props.contextMenu.visible()}
				onClose={props.contextMenu.close}
			/>
			<GitDialogOverlays contract={props.git} />
			<McpConfirmHost />
			<WorkflowApprovalHost />
			<PromptDialog
				visible={props.prompts.terminalRenameVisible()}
				title="Terminal Title"
				placeholder="Enter title"
				defaultValue={props.prompts.terminalRenameDefault()}
				confirmLabel="Rename"
				onClose={props.prompts.closeTerminalRename}
				onConfirm={(newName) => {
					const activeId = terminalsStore.state.activeId;
					if (activeId && newName !== props.prompts.terminalRenameDefault()) {
						terminalsStore.update(activeId, { name: newName, nameIsCustom: true });
					}
				}}
			/>
			<PromptDialog
				visible={props.prompts.openPathVisible()}
				title="Open Path"
				placeholder="Absolute path to file or folder"
				confirmLabel="Open"
				onClose={() => props.prompts.resolveOpenPath(null)}
				onConfirm={props.prompts.resolveOpenPath}
			/>
			<RemoteRepoPicker
				visible={props.prompts.repoPathVisible()}
				connectionName={window.location.host}
				onClose={() => props.prompts.resolveRepoPath(null)}
				onConfirm={props.prompts.resolveRepoPath}
			/>
			<Show when={props.prompts.remotePickerConnectionId()}>
				{(connectionId) => (
					<RemoteRepoPicker
						visible={true}
						connectionId={connectionId()}
						connectionName={remoteConnectionsStore.getConnections()[connectionId()]?.connection.name ?? connectionId()}
						onClose={() => props.prompts.resolveRemotePicker(null)}
						onConfirm={props.prompts.resolveRemotePicker}
					/>
				)}
			</Show>
			<ConfirmationOverlays contract={props.confirmations} />
			<Show when={props.utilities.processManagerVisible()}>
				<ProcessManagerModal onClose={props.utilities.closeProcessManager} />
			</Show>
			<Show when={props.utilities.generatorsVisible()}>
				<GeneratorsModal onClose={props.utilities.closeGenerators} />
			</Show>
			<Show when={props.utilities.remoteQrVisible()}>
				<RemoteQrDialog onClose={props.utilities.closeRemoteQr} />
			</Show>
			<WhatsNewDialog
				visible={props.utilities.whatsNewVersion() !== null && (whatsNewEntry()?.highlights.length ?? 0) > 0}
				version={props.utilities.whatsNewVersion() ?? ""}
				highlights={whatsNewEntry()?.highlights ?? []}
				contributions={whatsNewEntry()?.contributions ?? []}
				onClose={() => {
					const version = props.utilities.whatsNewVersion();
					if (version) invoke("set_last_seen_version", { version }).catch(() => {});
					props.utilities.setWhatsNewVersion(null);
				}}
			/>
			<UpdateProgressDialog />
			<CleanupOverlay contract={props.cleanup} />
			<Suspense>
				<HelpPanel visible={props.panels.helpVisible()} onClose={props.panels.closeHelp} />
			</Suspense>
			<CloseDialog />
		</>
	);
}
