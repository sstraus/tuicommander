import { createSignal } from "solid-js";
import { repositoriesStore } from "../../stores/repositories";
import { initMouseDrag } from "../../hooks/useMouseDrag";

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

  const updateHover = (x: number, y: number, payload: DragPayload) => {
    const el = document.elementFromPoint(x, y);
    const repoEl = el?.closest("[data-sidebar-repo]") as HTMLElement | null;
    const groupEl = el?.closest("[data-sidebar-group]") as HTMLElement | null;

    if (repoEl) {
      const path = repoEl.dataset.sidebarRepo!;
      if (payload.type === "repo" && path === payload.path) {
        setDragOverRepoPath(null);
        setDragOverSide(null);
        return;
      }
      const rect = repoEl.getBoundingClientRect();
      setDragOverRepoPath(path);
      setDragOverSide(y < rect.top + rect.height / 2 ? "top" : "bottom");
      setDragOverGroupId(null);
      setDragOverGroupSide(null);
    } else if (groupEl) {
      const gid = groupEl.dataset.sidebarGroup!;
      if (payload.type === "group" && gid === payload.groupId) {
        setDragOverGroupId(null);
        setDragOverGroupSide(null);
        return;
      }
      const rect = groupEl.getBoundingClientRect();
      setDragOverGroupId(gid);
      setDragOverGroupSide(y < rect.top + rect.height / 2 ? "top" : "bottom");
      setDragOverRepoPath(null);
      setDragOverSide(null);
    } else {
      setDragOverRepoPath(null);
      setDragOverSide(null);
      setDragOverGroupId(null);
      setDragOverGroupSide(null);
    }
  };

  const performDrop = (x: number, y: number, payload: DragPayload) => {
    const el = document.elementFromPoint(x, y);
    const repoEl = el?.closest("[data-sidebar-repo]") as HTMLElement | null;
    const groupEl = el?.closest("[data-sidebar-group]") as HTMLElement | null;

    if (repoEl && payload.type === "repo") {
      const targetPath = repoEl.dataset.sidebarRepo!;
      if (targetPath !== payload.path) {
        const rect = repoEl.getBoundingClientRect();
        const side = y < rect.top + rect.height / 2 ? "top" : "bottom";
        doRepoDrop(payload, targetPath, side);
      }
    } else if (groupEl) {
      const targetGroupId = groupEl.dataset.sidebarGroup!;
      if (payload.type === "group") {
        if (targetGroupId !== payload.groupId) {
          const rect = groupEl.getBoundingClientRect();
          const side = y < rect.top + rect.height / 2 ? "top" : "bottom";
          doGroupDrop(payload, targetGroupId, side);
        }
      } else if (payload.type === "repo") {
        repositoriesStore.addRepoToGroup(payload.path, targetGroupId);
      }
    }
    resetDragState();
  };

  const doRepoDrop = (payload: DragPayload & { type: "repo" }, targetPath: string, side: "top" | "bottom") => {
    const sourcePath = payload.path;
    const sourceGroupId = payload.fromGroupId;
    const targetGroup = repositoriesStore.getGroupForRepo(targetPath);
    const targetGroupId = targetGroup?.id ?? null;

    if (sourceGroupId === targetGroupId) {
      if (sourceGroupId) {
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
      repositoriesStore.removeRepoFromGroup(sourcePath);
    } else {
      if (sourceGroupId) {
        const targetGroupObj = repositoriesStore.state.groups[targetGroupId];
        const targetIndex = targetGroupObj ? targetGroupObj.repoOrder.indexOf(targetPath) : 0;
        let insertIndex = targetIndex;
        if (side === "bottom") insertIndex = targetIndex + 1;
        repositoriesStore.moveRepoBetweenGroups(sourcePath, sourceGroupId, targetGroupId, insertIndex);
      } else {
        repositoriesStore.addRepoToGroup(sourcePath, targetGroupId);
      }
    }
  };

  const doGroupDrop = (payload: DragPayload & { type: "group" }, targetGroupId: string, side: "top" | "bottom") => {
    const order = repositoriesStore.state.groupOrder;
    const fromIndex = order.indexOf(payload.groupId);
    const toIndex = order.indexOf(targetGroupId);
    if (fromIndex !== -1 && toIndex !== -1) {
      let adjustedTo = toIndex;
      if (side === "top" && fromIndex < toIndex) adjustedTo = toIndex - 1;
      else if (side === "bottom" && fromIndex > toIndex) adjustedTo = toIndex + 1;
      const clampedTo = Math.max(0, Math.min(adjustedTo, order.length - 1));
      if (fromIndex !== clampedTo) {
        repositoriesStore.reorderGroups(fromIndex, clampedTo);
      }
    }
  };

  // Mouse-based drag for repos
  const handleRepoMouseDrag = (e: MouseEvent, path: string) => {
    const fromGroup = repositoriesStore.getGroupForRepo(path);
    const payload: DragPayload = { type: "repo", path, fromGroupId: fromGroup?.id ?? null };
    initMouseDrag(e, e.currentTarget as HTMLElement, {
      onStart: () => setDragPayload(payload),
      onMove: (x, y) => updateHover(x, y, payload),
      onDrop: (x, y) => performDrop(x, y, payload),
      onCancel: () => resetDragState(),
    });
  };

  // Mouse-based drag for groups
  const handleGroupMouseDrag = (e: MouseEvent, groupId: string) => {
    const payload: DragPayload = { type: "group", groupId };
    initMouseDrag(e, e.currentTarget as HTMLElement, {
      onStart: () => setDragPayload(payload),
      onMove: (x, y) => updateHover(x, y, payload),
      onDrop: (x, y) => performDrop(x, y, payload),
      onCancel: () => resetDragState(),
    });
  };

  return {
    dragPayload,
    draggedRepoPath,
    dragOverRepoPath,
    dragOverSide,
    dragOverGroupId,
    dragOverGroupSide,
    resetDragState,
    handleRepoMouseDrag,
    handleGroupMouseDrag,
  };
}
