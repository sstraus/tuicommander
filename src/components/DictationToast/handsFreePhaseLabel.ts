import { t } from "../../i18n";
import type { HandsFreePhase } from "../../stores/dictation";

/** A hands-free phase in the user's words, shared by the toast and Settings. */
export function handsFreePhaseLabel(phase: HandsFreePhase | undefined): string {
	switch (phase) {
		case "waiting":
		case "capturing":
			return t("dictation.phaseWaiting", "Listening");
		case "wake_confirmed":
			return t("dictation.phaseWakeConfirmed", "Go ahead");
		case "transcribing":
			return t("dictation.phaseTranscribing", "Transcribing");
		case "holding_back":
			return t("dictation.phaseHoldingBack", "About to send");
		case "delivered":
			return t("dictation.phaseDelivered", "Sent");
		case "error":
			return t("dictation.phaseError", "Error");
		default:
			return t("dictation.phaseDisarmed", "Stopped");
	}
}
