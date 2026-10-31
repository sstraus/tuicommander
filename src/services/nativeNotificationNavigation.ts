import { listen } from "../invoke";
import { automationsUi } from "../stores/automations";
import { progressStore } from "../stores/progress";
import { uiStore } from "../stores/ui";
import { navigateToTerminal } from "../utils/navigateToTerminal";
import { openTaskConversation } from "./taskNavigation";

export type NativeNoticeTarget =
	| { kind: "automation"; id: string }
	| { kind: "terminal"; id: string }
	| { kind: "progress"; project: string; ptyId?: string | null }
	| { kind: "aichat"; id: string }
	| { kind: "pr"; url: string };

export function navigateFromNativeNotice(target: NativeNoticeTarget): void {
	if (target.kind === "automation") automationsUi.openRun(target.id);
	else if (target.kind === "terminal") navigateToTerminal(target.id);
	else if (target.kind === "aichat") uiStore.setAiChatPanelVisible(true);
	else if (target.kind === "pr") void openTaskConversation(target.url);
	else progressStore.open(target.project, target.ptyId ?? null);
}

export function listenForNativeNoticeClicks(): Promise<() => void> {
	return listen<NativeNoticeTarget>("native-notification-click", (event) => navigateFromNativeNotice(event.payload));
}
