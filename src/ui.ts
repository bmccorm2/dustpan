import { createElement, type IconNode } from "lucide";
import { api } from "./api";

type Attrs = Record<string, string | number | boolean | EventListener | undefined | null>;
type Child = Node | string | number | null | undefined | false;

/** Tiny DOM builder: el("div.a.b", { onclick }, child, ...). */
export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K | `${K}.${string}`,
  attrs: Attrs = {},
  ...children: (Child | Child[])[]
): HTMLElementTagNameMap[K] {
  const [name, ...classes] = tag.split(".");
  const node = document.createElement(name as K);
  if (classes.length) node.className = classes.join(" ");
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith("on") && typeof v === "function") {
      node.addEventListener(k.slice(2), v as EventListener);
    } else if (k === "class") {
      node.className = [node.className, String(v)].filter(Boolean).join(" ");
    } else if (k in node && typeof v !== "string") {
      (node as unknown as Record<string, unknown>)[k] = v;
    } else {
      node.setAttribute(k, v === true ? "" : String(v));
    }
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    node.append(typeof c === "object" ? c : String(c));
  }
  return node;
}

export function icon(node: IconNode, cls = "icon"): SVGElement {
  return createElement(node, { class: cls });
}

/** Bytes in base 1000, like Finder: "1.2 GB". */
export function formatBytes(n: number): string {
  if (n < 1000) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB", "PB"];
  let v = n;
  let i = -1;
  do {
    v /= 1000;
    i++;
  } while (v >= 1000 && i < units.length - 1);
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

let toastHost: HTMLElement | null = null;

/** Short message in the bottom-right corner. */
export function toast(message: string, kind: "info" | "error" = "info", ms = 4000): void {
  if (!toastHost) {
    toastHost = el("div.toasts");
    document.body.append(toastHost);
  }
  const t = el("div.toast", { class: kind === "error" ? "toast-error" : "" }, message);
  toastHost.append(t);
  setTimeout(() => t.remove(), kind === "error" ? ms * 2 : ms);
}

/** Error toast for a failed invoke(). */
export function toastError(e: unknown): void {
  toast(typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e), "error");
}

/** Modal confirm. Resolves true only when the confirm button is pressed. */
export function confirmDialog(opts: {
  title: string;
  message: string | Node;
  confirmLabel: string;
  danger?: boolean;
}): Promise<boolean> {
  return new Promise((resolve) => {
    const dlg = el("dialog.confirm");
    const done = (v: boolean) => {
      dlg.close();
      dlg.remove();
      resolve(v);
    };
    const ok = el("button.btn.primary", { onclick: () => done(true) }, opts.confirmLabel);
    dlg.append(
      el("h3", {}, opts.title),
      el("div.confirm-body", {}, opts.message),
      el(
        "div.confirm-actions",
        {},
        el("button.btn", { onclick: () => done(false) }, "Cancel"),
        ok,
      ),
    );
    dlg.addEventListener("cancel", (e) => {
      e.preventDefault();
      done(false);
    });
    document.body.append(dlg);
    dlg.showModal();
    ok.focus();
  });
}

/** Banner shown while Full Disk Access is missing. Re-checks when the window regains focus. */
export function fdaBanner(): HTMLElement {
  const banner = el(
    "div.banner-info.fda-banner",
    { hidden: true },
    el(
      "span",
      {},
      "Dustpan doesn't have Full Disk Access, so some caches and app data can't be read. ",
    ),
    el("button.btn", { onclick: () => api.openFdaSettings().catch(toastError) }, "Open System Settings"),
  );
  const refresh = () =>
    api
      .fdaStatus()
      .then((ok) => (banner.hidden = ok))
      .catch(() => (banner.hidden = true));
  refresh();
  window.addEventListener("focus", refresh);
  return banner;
}

/** Disables a button while an async action runs. */
export async function busy<T>(btn: HTMLButtonElement, fn: () => Promise<T>): Promise<T | undefined> {
  btn.disabled = true;
  try {
    return await fn();
  } catch (e) {
    toastError(e);
    return undefined;
  } finally {
    btn.disabled = false;
  }
}
