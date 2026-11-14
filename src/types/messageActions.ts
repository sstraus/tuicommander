/** Conversational marks, not ratings or tool approvals. Rust owns their accepted state. */
export type MessageActionReaction = "thumbs_up" | "thumbs_down" | "heart" | "celebrate";
export type MessageAction =
	| { kind: "reaction"; reaction: MessageActionReaction; remove: boolean }
	| { kind: "comment"; comment: string };
export type MessageReactions = Record<string, MessageActionReaction[]>;
export interface MessageActions {
	reactions: () => MessageReactions;
	send: (messageId: string, text: string, action: MessageAction) => Promise<void>;
}
