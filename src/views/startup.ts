import { FolderSearch, RefreshCw, Trash2 } from "lucide";
import { api, type StartupItem, type StartupKind } from "../api";
import { busy, confirmDialog, el, formatBytes, icon, toast, toastError } from "../ui";

type Filter = "all" | "enabled" | "disabled" | "broken";

const GROUPS: { kind: StartupKind; label: string }[] = [
  { kind: "loginItem", label: "Login items" },
  { kind: "userAgent", label: "My background services" },
  { kind: "globalAgent", label: "System agents" },
  { kind: "globalDaemon", label: "System daemons" },
];

let items: StartupItem[] = [];
let warning: string | null = null;
let filter: Filter = "all";
let body: HTMLElement;
let warnBox: HTMLElement;
let filterSeg: HTMLElement;

export function mount(root: HTMLElement): void {
  filterSeg = el("div.segmented.filter");
  for (const f of ["all", "enabled", "disabled", "broken"] as Filter[]) {
    filterSeg.append(
      el(
        "button",
        {
          "data-id": f,
          onclick: () => {
            filter = f;
            render();
          },
        },
        f[0].toUpperCase() + f.slice(1),
      ),
    );
  }
  const reload = el("button.icon-btn", { title: "Refresh", onclick: () => busy(reload, load) }, icon(RefreshCw));
  warnBox = el("div.banner-info.warn-banner", { hidden: true });
  body = el("div.scroll");
  root.append(el("header.toolbar", {}, filterSeg, el("span.spacer"), reload), warnBox, body);
  body.append(el("div.center", {}, el("span.spinner"), el("div.dim", {}, "Reading startup items…")));
  load();
}

async function load() {
  try {
    const r = await api.startupList();
    items = r.items;
    warning = r.warning;
  } catch (e) {
    toastError(e);
  }
  render();
}

function visible(i: StartupItem): boolean {
  switch (filter) {
    case "enabled":
      return i.enabled;
    case "disabled":
      return !i.enabled;
    case "broken":
      return i.broken;
    default:
      return true;
  }
}

function render() {
  for (const b of filterSeg.children) b.classList.toggle("on", (b as HTMLElement).dataset.id === filter);
  warnBox.hidden = !warning;
  warnBox.textContent = warning ?? "";
  body.replaceChildren();
  for (const g of GROUPS) {
    const list = items.filter((i) => i.kind === g.kind && visible(i));
    body.append(el("div.label.list-label", {}, `${g.label} (${list.length})`));
    body.append(
      list.length
        ? el("div.section", {}, list.map(row))
        : el("div.empty.small", {}, filter === "all" ? "None." : `No ${filter} items.`),
    );
  }
}

function row(i: StartupItem): HTMLElement {
  const sub = i.program ?? i.path;
  const toggle = el("input.switch", {
    type: "checkbox",
    checked: i.enabled,
    title: i.enabled ? "Enabled" : "Disabled",
    onchange: async () => {
      toggle.disabled = true;
      try {
        await api.startupSetEnabled(i.id, toggle.checked);
        i.enabled = toggle.checked;
        toast(`${i.label} ${i.enabled ? "enabled" : "disabled"}`);
      } catch (e) {
        toggle.checked = i.enabled;
        toastError(e);
      } finally {
        toggle.disabled = false;
        render();
      }
    },
  }) as HTMLInputElement;
  const remove = el("button.icon-btn", { title: "Remove", onclick: () => removeItem(i, remove) }, icon(Trash2));
  const canToggle = i.editable && i.kind === "userAgent";
  return el(
    "div.item.startup",
    { class: i.enabled ? "" : "off" },
    el(
      "div.startup-text",
      {},
      el("div", {}, el("span", {}, i.label), i.broken ? el("span.badge-error", {}, "Broken") : null, i.enabled ? null : el("span.badge", {}, "Disabled")),
      sub ? el("div.mono.sub", { title: sub }, sub) : null,
    ),
    el("span.spacer"),
    i.path ? el("button.icon-btn", { title: "Reveal in Finder", onclick: () => api.reveal(i.path).catch(toastError) }, icon(FolderSearch)) : null,
    i.editable ? remove : null,
    canToggle ? toggle : null,
  );
}

async function removeItem(i: StartupItem, btn: HTMLButtonElement) {
  const login = i.kind === "loginItem";
  const ok = await confirmDialog({
    title: `Remove ${i.label}?`,
    message: login
      ? "Removes it from your login items. The app itself stays installed."
      : `Disables it and moves 1 item (${i.path.split("/").pop()}) to the Trash.`,
    confirmLabel: "Remove",
  });
  if (!ok) return;
  await busy(btn, async () => {
    const r = await api.startupRemove(i.id);
    if (r.failed.length) toast(r.failed[0].reason, "error");
    else toast(`Removed ${i.label}${r.bytes ? ` (${formatBytes(r.bytes)})` : ""}`);
    await load();
  });
}
