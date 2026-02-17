import { Component } from "solid-js";

export type BadgeVariant = "default" | "success" | "warning" | "error" | "info" | "branch" | "pr" | "ci" | "merged" | "closed";

export interface StatusBadgeProps {
  label: string;
  variant?: BadgeVariant;
  title?: string;
  onClick?: () => void;
}

export const StatusBadge: Component<StatusBadgeProps> = (props) => {
  const variant = () => props.variant || "default";

  return (
    <span
      class={`status-badge ${variant()}`}
      title={props.title}
      onClick={props.onClick}
      style={{ cursor: props.onClick ? "pointer" : "default" }}
    >
      {props.label}
    </span>
  );
};

/** GitHub branch status badge */
export interface BranchBadgeProps {
  branch: string;
  ahead: number;
  behind: number;
  onClick?: () => void;
}

export const BranchBadge: Component<BranchBadgeProps> = (props) => {
  const label = () => {
    let text = `⎇ ${props.branch}`;
    if (props.ahead > 0 && props.behind > 0) {
      text += ` ↑${props.ahead} ↓${props.behind}`;
    } else if (props.ahead > 0) {
      text += ` ↑${props.ahead}`;
    } else if (props.behind > 0) {
      text += ` ↓${props.behind}`;
    }
    return text;
  };

  const variant = () => (props.ahead > 0 ? "info" : "branch");
  const title = () => (props.onClick ? "Click to rename branch" : props.branch);

  return (
    <StatusBadge
      label={label()}
      variant={variant()}
      title={title()}
      onClick={props.onClick}
    />
  );
};

/** PR status badge */
export interface PrBadgeProps {
  number: number;
  title: string;
  state: string;
  mergeable?: string;
  mergeStateStatus?: string;
  onClick?: () => void;
}

export const PrBadge: Component<PrBadgeProps> = (props) => {
  const variant = (): BadgeVariant => {
    switch (props.state.toLowerCase()) {
      case "merged":
        return "merged";
      case "closed":
        return "closed";
      default:
        // Show warning/error for merge problems on open PRs
        if (props.mergeable === "CONFLICTING") return "error";
        if (props.mergeStateStatus === "BEHIND" || props.mergeStateStatus === "BLOCKED" || props.mergeStateStatus === "UNSTABLE") return "warning";
        return "pr";
    }
  };

  const titleText = () => props.onClick ? `${props.title} (click to open)` : props.title;

  return (
    <StatusBadge
      label={`PR #${props.number}`}
      variant={variant()}
      title={titleText()}
      onClick={props.onClick}
    />
  );
};

/** CI status badge */
export interface CiBadgeProps {
  status: string;
  conclusion: string | null;
  workflowName: string;
}

export const CiBadge: Component<CiBadgeProps> = (props) => {
  const state = () => props.conclusion || props.status;

  const variant = (): BadgeVariant => {
    switch (state().toLowerCase()) {
      case "success":
        return "success";
      case "failure":
        return "error";
      case "pending":
      case "queued":
        return "warning";
      default:
        return "ci";
    }
  };

  const label = () => {
    switch (state().toLowerCase()) {
      case "success":
        return "CI passed";
      case "failure":
        return "CI failed";
      default:
        return `CI ${state()}`;
    }
  };

  return (
    <StatusBadge label={label()} variant={variant()} title={props.workflowName} />
  );
};
