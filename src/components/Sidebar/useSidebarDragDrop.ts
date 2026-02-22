import { createSignal } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";

export type DragPayload =
  | { type: "repo"; path: string; fromGroupId: string | null }
  | { type: "group"; groupId: string };

export function useSidebarDragDrop() {
  const [dragPayload, setDragPayload] = createSignal<DragPayload | null>(null);
  const [dragOverRepoPath, setDragOverRepoPath] = createSignal<string | null>(null);
  const [dragOverSide, setDragOverSide] = createSignal<"top" | "bottom" | null>(null);
  const [dragOverGroupId, setDragOverGroupId] = createSignal<string | null>(null);
  const [dragOverGroupSide, setDragOverGroupSide] = createSignal<"top" | "bottom" | null>(null);

  const draggedRepoPath = () => {
    const p = dragPayload();
    return p?.type === "repo" ? p.path : null;
  };

  const resetDragState = () => {
    setDragPayload(null);
    setDragOverRepoPath(null);
    setDragOverSide(null);
    setDragOverGroupId(null);
    setDragOverGroupSide(null);
  };

  const handleRepoDragStart = (e: DragEvent, path: string) => {
    if (!e.dataTransfer) return;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", `repo:${path}`);
    try {
      const el = e.currentTarget as HTMLElement;
      e.dataTransfer.setDragImage(el, el.offsetWidth / 2, el.offsetHeight / 2);
    } catch { /* not supported in all envs */ }
    const fromGroup = repositoriesStore.getGroupForRepo(path);
    setDragPayload({ type: "repo", path, fromGroupId: fromGroup?.id ?? null });
  };

  const handleRepoDragOver = (e: DragEvent, path: string) => {
    e.preventDefault();
    if (!e.dataTransfer) return;
    e.dataTransfer.dropEffect = "move";
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const midpoint = rect.top + rect.height / 2;
    setDragOverRepoPath(path);
    setDragOverSide(e.clientY < midpoint ? "top" : "bottom");
    // Clear group-level hover when hovering a repo
    setDragOverGroupId(null);
    setDragOverGroupSide(null);
  };

  const handleRepoDrop = (e: DragEvent, targetPath: string) => {
    e.preventDefault();
    const payload = dragPayload();
    if (!payload || payload.type !== "repo" || payload.path === targetPath) {
      resetDragState();
      return;
    }

    const sourcePath = payload.path;
    const sourceGroupId = payload.fromGroupId;
    const targetGroup = repositoriesStore.getGroupForRepo(targetPath);
    const targetGroupId = targetGroup?.id ?? null;
    const side = dragOverSide();

    if (sourceGroupId === targetGroupId) {
      // Same context (same group or both ungrouped) — reorder
      if (sourceGroupId) {
        // Reorder within group
        const group = repositoriesStore.state.groups[sourceGroupId];
        if (group) {
          const fromIndex = group.repoOrder.indexOf(sourcePath);
          const toIndex = group.repoOrder.indexOf(targetPath);
          if (fromIndex !== -1 && toIndex !== -1 && fromIndex !== toIndex) {
            let adjustedTo = toIndex;
            if (side === "top" && fromIndex < toIndex) adjustedTo = toIndex - 1;
            else if (side === "bottom" && fromIndex > toIndex) adjustedTo = toIndex + 1;
            const clampedTo = Math.max(0, Math.min(adjustedTo, group.repoOrder.length - 1));
            if (fromIndex !== clampedTo) {
              repositoriesStore.reorderRepoInGroup(sourceGroupId, fromIndex, clampedTo);
            }
          }
        }
      } else {
        // Reorder within ungrouped
        const order = repositoriesStore.state.repoOrder;
        const fromIndex = order.indexOf(sourcePath);
        const toIndex = order.indexOf(targetPath);
        if (fromIndex !== -1 && toIndex !== -1 && fromIndex !== toIndex) {
          let adjustedTo = toIndex;
          if (side === "top" && fromIndex < toIndex) adjustedTo = toIndex - 1;
          else if (side === "bottom" && fromIndex > toIndex) adjustedTo = toIndex + 1;
          const clampedTo = Math.max(0, Math.min(adjustedTo, order.length - 1));
          if (fromIndex !== clampedTo) {
            repositoriesStore.reorderRepo(fromIndex, clampedTo);
          }
        }
      }
    } else if (targetGroupId === null) {
      // Dragged from group to ungrouped area — remove from group
      repositoriesStore.removeRepoFromGroup(sourcePath);
    } else {
      // Dragged to different group
      if (sourceGroupId) {
        // Between two groups — preserves insert position
        const targetGroupObj = repositoriesStore.state.groups[targetGroupId];
        const targetIndex = targetGroupObj ? targetGroupObj.repoOrder.indexOf(targetPath) : 0;
        let insertIndex = targetIndex;
        if (side === "bottom") insertIndex = targetIndex + 1;
        repositoriesStore.moveRepoBetweenGroups(sourcePath, sourceGroupId, targetGroupId, insertIndex);
      } else {
        // From ungrouped into a group
        repositoriesStore.addRepoToGroup(sourcePath, targetGroupId);
      }
    }

    resetDragState();
  };

  // --- Group-level drag handlers ---
  const handleGroupDragStart = (e: DragEvent, groupId: string) => {
    if (!e.dataTransfer) return;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", `group:${groupId}`);
    try {
      const el = e.currentTarget as HTMLElement;
      e.dataTransfer.setDragImage(el, el.offsetWidth / 2, el.offsetHeight / 2);
    } catch { /* not supported in all envs */ }
    setDragPayload({ type: "group", groupId });
  };

  const handleGroupDragOver = (e: DragEvent, groupId: string) => {
    e.preventDefault();
    if (!e.dataTransfer) return;
    e.dataTransfer.dropEffect = "move";
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const midpoint = rect.top + rect.height / 2;
    setDragOverGroupId(groupId);
    setDragOverGroupSide(e.clientY < midpoint ? "top" : "bottom");
  };

  const handleGroupDrop = (e: DragEvent, targetGroupId: string) => {
    e.preventDefault();
    const payload = dragPayload();
    if (!payload) {
      resetDragState();
      return;
    }

    if (payload.type === "group") {
      // Group-to-group reorder
      if (payload.groupId !== targetGroupId) {
        const order = repositoriesStore.state.groupOrder;
        const fromIndex = order.indexOf(payload.groupId);
        const toIndex = order.indexOf(targetGroupId);
        if (fromIndex !== -1 && toIndex !== -1) {
          let adjustedTo = toIndex;
          const side = dragOverGroupSide();
          if (side === "top" && fromIndex < toIndex) adjustedTo = toIndex - 1;
          else if (side === "bottom" && fromIndex > toIndex) adjustedTo = toIndex + 1;
          const clampedTo = Math.max(0, Math.min(adjustedTo, order.length - 1));
          if (fromIndex !== clampedTo) {
            repositoriesStore.reorderGroups(fromIndex, clampedTo);
          }
        }
      }
    } else if (payload.type === "repo") {
      // Repo dropped on group header — assign to group
      repositoriesStore.addRepoToGroup(payload.path, targetGroupId);
    }

    resetDragState();
  };

  return {
    dragPayload,
    draggedRepoPath,
    dragOverRepoPath,
    dragOverSide,
    dragOverGroupId,
    dragOverGroupSide,
    resetDragState,
    handleRepoDragStart,
    handleRepoDragOver,
    handleRepoDrop,
    handleGroupDragStart,
    handleGroupDragOver,
    handleGroupDrop,
  };
}
