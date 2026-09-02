import { useCallback, useState } from "react";
import { organizeRootByPeople } from "../api";
import type { RootInfo } from "../types";
import { errorMessage } from "../utils";

type OrganizeCallbacks = {
  onNotice: (msg: string) => void;
  onError: (msg: string) => void;
  /** Called after files were moved so listings, tree and stats re-query. */
  onChanged: () => Promise<void> | void;
};

/** "Organize by people": confirm target + run the on-disk reorganisation. */
export function useOrganizeManager(cb: OrganizeCallbacks) {
  const [organizeTarget, setOrganizeTarget] = useState<RootInfo | null>(null);
  const [busy, setBusy] = useState(false);

  const cancel = useCallback(() => setOrganizeTarget(null), []);

  const onConfirm = useCallback(async () => {
    const root = organizeTarget;
    if (!root) return;
    setBusy(true);
    try {
      const r = await organizeRootByPeople(root.id);
      setOrganizeTarget(null);
      const parts = [`${r.moved} moved`, `${r.copied} copied`, `${r.people} people`];
      if (r.skipped > 0) parts.push(`${r.skipped} already in place`);
      if (r.errors.length > 0) {
        cb.onError(`Organized "${root.rootName}" with ${r.errors.length} error(s): ${r.errors[0]}`);
      } else {
        cb.onNotice(`Organized "${root.rootName}": ${parts.join(", ")}`);
      }
      await cb.onChanged();
    } catch (err) {
      cb.onError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [organizeTarget, cb]);

  return { organizeTarget, setOrganizeTarget, busy, cancel, onConfirm };
}
