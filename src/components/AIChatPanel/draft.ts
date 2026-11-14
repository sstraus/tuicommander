/** AI Chat uses the shared prompt draft owner with its existing storage keys. */
import { createPromptDraft } from "../../stores/promptDraft";

export { MAX_PASTED_IMAGE_BYTES, type StagedFile, type StagedImage } from "../../stores/promptDraft";
export const aiChatDraft = createPromptDraft();
