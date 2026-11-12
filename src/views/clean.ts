import { listen } from "@tauri-apps/api/event";
import { ChevronDown, ChevronRight, FolderSearch, Sparkles, Trash2 } from "lucide";
import { api, type ActionResult, type Cat, type CleanItem } from "../api";
import { busy, confirmDialog, el, formatBytes, icon, toast, toastError } from "../ui";

const CATS: { id: Cat; label: string }[] = [
  { id: "appCaches", label: "App caches" },
  { id: "browsers", label: "Browsers" },
  { id: "developer", label: "Developer tools" },
  { id: "ai", label: "AI tools" },
  { id: "logs", label: "Logs" },
  { id: "leftovers", label: "Leftovers" },
  { id: "trash", label: "Trash" },
];
const MAX_PATHS = 200;

let root: HTMLElement;
let items: CleanItem[] = [];
const selected = new Set<string>();
const collapsed = new Set<Cat>();

export function mount(r: HTMLElement): void {
  root = r;
  root.classList.add("clean");
  renderIdle();
}

function renderIdle() {
  const btn = el("button.btn.primary.big", { onclick: () => scan() }, icon(Sparkles), "Scan");
  root.replaceChildren(
    el(
      "div.center",
      {},
      el("h2", {}, "Clean up your Mac"),
      el("p.dim", {}, "Finds caches, logs, developer and AI tool junk, leftovers of removed apps, and the Trash."),
      btn,
    ),
  );
}

async function scan() {
  const label = el("div.dim", {}, "Starting…");
  root.replaceChildren(el("div.center", {}, el("span.spinner"), el("div", {}, "Scanning"), label));
  const unlisten = await listen<{ label: string }>("clean://progress", (e) => {
    label.textContent = e.payload.label;
  });
  try {
    items = await api.cleanScan();
    selected.clear();
    preset("recommended");
    renderResults();
  } catch (e) {
    toastError(e);
    renderIdle();
  } finally {
    unlisten();
  }
}

function preset(kind: "recommended" | "all" | "none") {
  selected.clear();
  for (const i of items) {
    if (i.blockedReason) continue;
    if (kind === "all" || (kind === "recommended" && i.recommended)) selected.add(i.id);
  }
}

function selectedItems(): CleanItem[] {
  return items.filter((i) => selected.has(i.id) && !i.blockedReason);
}

function renderResults() {
  const total = el("div.total");
  const go = el("button.btn.primary", { onclick: () => clean(go) });
  const refreshHeader = () => {
    const sel = selectedItems();
    const bytes = sel.reduce((a, i) => a + i.size, 0);
    total.replaceChildren(
      el("span.big-num", {}, formatBytes(bytes)),
      el("span.dim", {}, ` selected · ${sel.length} of ${items.length} items`),
    );
    go.replaceChildren(icon(Trash2), `Move to Trash · ${formatBytes(bytes)}`);
    go.disabled = sel.length === 0;
  };

  const link = (label: string, kind: "recommended" | "all" | "none") =>
    el(
      "button.link",
      {
        onclick: () => {
          preset(kind);
          renderResults();
        },
      },
      label,
    );

  const header = el(
    "header.toolbar",
    {},
    total,
    el("div.presets", {}, "Select: ", link("Recommended", "recommended"), link("All", "all"), link("None", "none")),
    el("div.spacer"),
    el("button.btn", { onclick: () => scan() }, "Rescan"),
    go,
  );

  const list = el("div.scroll");
  for (const cat of CATS) {
    const catItems = items.filter((i) => i.category === cat.id).sort((a, b) => b.size - a.size);
    if (!catItems.length) continue;
    list.append(section(cat.id, cat.label, catItems, refreshHeader));
  }
  if (!list.childElementCount) list.append(el("div.empty", {}, "Nothing to clean. Your Mac is tidy."));

  root.replaceChildren(header, list);
  refreshHeader();
}

function section(cat: Cat, label: string, catItems: CleanItem[], onChange: () => void): HTMLElement {
  const sum = catItems.reduce((a, i) => a + i.size, 0);
  const body = el("div.section-body", { hidden: collapsed.has(cat) });
  const chev = el("span.chev", {}, icon(collapsed.has(cat) ? ChevronRight : ChevronDown));
  const selectable = catItems.filter((i) => !i.blockedReason);
  const all = el("input", {
    type: "checkbox",
    disabled: selectable.length === 0,
    onclick: (e: Event) => e.stopPropagation(),
    onchange: () => {
      for (const i of selectable) (all.checked ? selected.add(i.id) : selected.delete(i.id));
      for (const cb of body.querySelectorAll<HTMLInputElement>("input.item-check:not(:disabled)")) cb.checked = all.checked;
      onChange();
    },
  }) as HTMLInputElement;
  const syncAll = () => {
    const n = selectable.filter((i) => selected.has(i.id)).length;
    all.checked = n > 0 && n === selectable.length;
    all.indeterminate = n > 0 && n < selectable.length;
  };
  syncAll();

  const head = el(
    "div.section-head",
    {
      onclick: () => {
        const hide = !body.hidden;
        body.hidden = hide;
        hide ? collapsed.add(cat) : collapsed.delete(cat);
        chev.replaceChildren(icon(hide ? ChevronRight : ChevronDown));
      },
    },
    chev,
    all,
    el("span.section-title", {}, label),
    el("span.dim", {}, `${catItems.length}`),
    el("span.spacer"),
    el("span.size", {}, formatBytes(sum)),
  );

  for (const i of catItems) {
    body.append(
      itemRow(i, () => {
        syncAll();
        onChange();
      }),
    );
  }
  return el("div.section", {}, head, body);
}

function itemRow(i: CleanItem, onChange: () => void): HTMLElement {
  const paths = el("div.paths", { hidden: true });
  const chev = el("button.icon-btn", { title: "Show paths" }, icon(ChevronRight));
  chev.addEventListener("click", () => {
    paths.hidden = !paths.hidden;
    chev.replaceChildren(icon(paths.hidden ? ChevronRight : ChevronDown));
    if (!paths.hidden && !paths.childElementCount) fillPaths(paths, i.paths);
  });
  const cb = el("input.item-check", {
    type: "checkbox",
    checked: selected.has(i.id),
    disabled: !!i.blockedReason,
    onchange: () => {
      cb.checked ? selected.add(i.id) : selected.delete(i.id);
      onChange();
    },
  }) as HTMLInputElement;
  const row = el(
    "div.item",
    { class: i.blockedReason ? "blocked" : "" },
    chev,
    el("label.item-label", {}, cb, el("span", {}, i.label)),
    i.blockedReason ? el("span.reason", {}, i.blockedReason) : null,
    el("span.spacer"),
    el("span.dim.count", {}, i.paths.length === 1 ? "" : `${i.paths.length} items`),
    el("span.size", {}, formatBytes(i.size)),
  );
  return el("div", {}, row, paths);
}

function fillPaths(box: HTMLElement, paths: string[]) {
  for (const p of paths.slice(0, MAX_PATHS)) {
    box.append(
      el(
        "div.path",
        {},
        el("span.mono", { title: p }, p),
        el("button.icon-btn", { title: "Reveal in Finder", onclick: () => api.reveal(p).catch(toastError) }, icon(FolderSearch)),
      ),
    );
  }
  if (paths.length > MAX_PATHS) box.append(el("div.path.dim", {}, `…and ${paths.length - MAX_PATHS} more`));
}

async function clean(btn: HTMLButtonElement) {
  const sel = selectedItems();
  const toTrash = sel.filter((i) => i.category !== "trash");
  const bytes = toTrash.reduce((a, i) => a + i.size, 0);
  const emptying = sel.find((i) => i.category === "trash");
  const msg = el(
    "div",
    {},
    el("p", {}, `Move ${toTrash.length} item${toTrash.length === 1 ? "" : "s"} (${formatBytes(bytes)}) to the Trash?`),
    emptying
      ? el("p.text-error", {}, `Trash (${formatBytes(emptying.size)}) is selected: this permanently deletes everything in the Trash.`)
      : null,
  );
  const ok = await confirmDialog({ title: "Clean selected items", message: msg, confirmLabel: "Move to Trash" });
  if (!ok) return;
  await busy(btn, async () => {
    const paths = toTrash.flatMap((i) => i.paths);
    const res: ActionResult = paths.length ? await api.trashPaths(paths) : { done: [], failed: [], bytes: 0 };
    let emptied: ActionResult | null = null;
    if (emptying) emptied = await api.emptyTrash();
    await renderDone(res, emptied);
  });
}

async function renderDone(res: ActionResult, emptied: ActionResult | null) {
  const failed = [...res.failed, ...(emptied?.failed ?? [])];
  const failList = el(
    "div.fail-list",
    { hidden: true },
    failed.map((f) => el("div.fail", {}, el("span.mono", { title: f.path }, f.path), el("span.text-error", {}, f.reason))),
  );
  const emptyBtn = el("button.btn", { onclick: () => emptyTrash(emptyBtn) }, icon(Trash2), "Empty Trash");
  const view = el(
    "div.center.done",
    {},
    el("h2", {}, `Moved ${formatBytes(res.bytes)} to Trash`),
    emptied ? el("p", {}, `Permanently deleted ${formatBytes(emptied.bytes)} from the Trash.`) : null,
    el("p.dim", {}, "Space is freed once the Trash is emptied."),
    failed.length
      ? el(
          "button.link.text-error",
          { onclick: () => (failList.hidden = !failList.hidden) },
          `${failed.length} item${failed.length === 1 ? "" : "s"} failed`,
        )
      : null,
    failList,
    el("div.row-actions", {}, el("button.btn", { onclick: () => scan() }, "Scan again"), emptyBtn),
  );
  root.replaceChildren(view);
  await updateEmptyBtn(emptyBtn);
}

async function updateEmptyBtn(btn: HTMLButtonElement) {
  try {
    const size = await api.trashSize();
    btn.replaceChildren(icon(Trash2), `Empty Trash (${formatBytes(size)})`);
    btn.disabled = size === 0;
  } catch {
    btn.replaceChildren(icon(Trash2), "Empty Trash");
  }
}

async function emptyTrash(btn: HTMLButtonElement) {
  const ok = await confirmDialog({
    title: "Empty Trash",
    message: "This permanently deletes everything in the Trash.",
    confirmLabel: "Empty Trash",
  });
  if (!ok) return;
  await busy(btn, async () => {
    const r = await api.emptyTrash();
    toast(
      `Deleted ${formatBytes(r.bytes)} from the Trash` + (r.failed.length ? ` · ${r.failed.length} failed` : ""),
      r.failed.length ? "error" : "info",
    );
  });
  await updateEmptyBtn(btn);
}
