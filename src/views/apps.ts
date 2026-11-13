import { AppWindow, FolderSearch, RefreshCw, Search, Trash2 } from "lucide";
import { api, type AppInfo, type Leftover } from "../api";
import { busy, confirmDialog, el, formatBytes, icon, toast, toastError } from "../ui";
import * as startup from "./startup";

const SEG_KEY = "dustpan.appsSegment";

export function mount(root: HTMLElement): void {
  root.classList.add("apps");
  const uninstallPane = el("div.pane");
  const startupPane = el("div.pane");
  const mounted = new Set<string>();
  const seg = el("div.segmented.seg-top");
  const segs = [
    { id: "uninstall", label: "Uninstall", pane: uninstallPane, mount: mountUninstall },
    { id: "startup", label: "Startup", pane: startupPane, mount: startup.mount },
  ];
  const show = (id: string) => {
    const s = segs.find((x) => x.id === id) ?? segs[0];
    localStorage.setItem(SEG_KEY, s.id);
    if (!mounted.has(s.id)) {
      mounted.add(s.id);
      s.mount(s.pane);
    }
    for (const x of segs) x.pane.hidden = x !== s;
    for (const b of seg.children) b.classList.toggle("on", (b as HTMLElement).dataset.id === s.id);
  };
  for (const s of segs) seg.append(el("button", { "data-id": s.id, onclick: () => show(s.id) }, s.label));
  root.append(el("div.seg-bar", {}, seg), uninstallPane, startupPane);
  show(localStorage.getItem(SEG_KEY) ?? "uninstall");
}

// ---------- Uninstall ----------

let apps: AppInfo[] = [];
let sortBy: "name" | "size" = "name";
let query = "";
let current: AppInfo | null = null;
let listBox: HTMLElement;
let detail: HTMLElement;

function mountUninstall(root: HTMLElement) {
  const search = el("input.input.search", {
    type: "search",
    placeholder: "Search apps",
    oninput: () => {
      query = search.value.trim().toLowerCase();
      renderList();
    },
  }) as HTMLInputElement;
  const sortBtn = el("button.btn", {
    title: "Sort",
    onclick: () => {
      sortBy = sortBy === "name" ? "size" : "name";
      sortBtn.textContent = `Sort: ${sortBy === "name" ? "Name" : "Size"}`;
      renderList();
    },
  }, "Sort: Name");
  const reload = el("button.icon-btn", { title: "Refresh", onclick: () => busy(reload, load) }, icon(RefreshCw));
  listBox = el("div.app-list");
  detail = el("div.app-detail", {}, el("div.empty", {}, "Select an app to see what it installed."));
  root.append(
    el(
      "div.split",
      {},
      el(
        "div.app-side",
        {},
        el("div.app-tools", {}, el("label.search-wrap", {}, icon(Search), search), sortBtn, reload),
        listBox,
      ),
      detail,
    ),
  );
  listBox.append(el("div.center", {}, el("span.spinner"), el("div.dim", {}, "Finding apps…")));
  load();
}

async function load() {
  try {
    apps = await api.appsList();
  } catch (e) {
    toastError(e);
    apps = [];
  }
  if (current && !apps.some((a) => a.path === current!.path)) {
    current = null;
    detail.replaceChildren(el("div.empty", {}, "Select an app to see what it installed."));
  }
  renderList();
}

function appIcon(a: AppInfo, cls = "app-icon") {
  return a.icon ? el(`img.${cls}`, { src: a.icon, alt: "" }) : el(`span.${cls}.no-icon`, {}, icon(AppWindow));
}

function renderList() {
  let list = apps.filter((a) => !query || a.name.toLowerCase().includes(query) || a.bundleId.toLowerCase().includes(query));
  list = [...list].sort((a, b) => (sortBy === "size" ? b.size - a.size : a.name.localeCompare(b.name)));
  listBox.replaceChildren();
  if (!list.length) {
    listBox.append(el("div.empty", {}, apps.length ? "No apps match your search." : "No apps found."));
    return;
  }
  for (const a of list) {
    const row = el(
      "div.app-row",
      { class: current?.path === a.path ? "current" : "", onclick: () => select(a) },
      appIcon(a),
      el("div.app-text", {}, el("div.app-name", {}, a.name), el("div.app-ver", {}, a.version)),
      el("span.size", {}, formatBytes(a.size)),
    );
    listBox.append(row);
  }
}

async function select(a: AppInfo) {
  current = a;
  renderList();
  detail.replaceChildren(el("div.center", {}, el("span.spinner"), el("div.dim", {}, "Looking for app data…")));
  let leftovers: Leftover[];
  try {
    leftovers = await api.appLeftovers(a.bundleId, a.name);
  } catch (e) {
    toastError(e);
    leftovers = [];
  }
  if (current !== a) return;
  renderDetail(a, leftovers);
}

function renderDetail(a: AppInfo, leftovers: Leftover[]) {
  const checked = new Set<string>([a.path, ...leftovers.map((l) => l.path)]);
  const sizeOf = new Map<string, number>([[a.path, a.size], ...leftovers.map((l) => [l.path, l.size] as [string, number])]);
  const go = el("button.btn.primary", { onclick: () => uninstall(a, leftovers, checked, go) });
  const update = () => {
    const total = [...checked].reduce((s, p) => s + (sizeOf.get(p) ?? 0), 0);
    go.replaceChildren(icon(Trash2), `${checked.has(a.path) ? "Uninstall" : "Remove data"} · ${formatBytes(total)}`);
    go.disabled = checked.size === 0;
  };

  const row = (path: string, label: string, kind: string, size: number) => {
    const cb = el("input", {
      type: "checkbox",
      checked: true,
      onchange: () => {
        cb.checked ? checked.add(path) : checked.delete(path);
        update();
      },
    }) as HTMLInputElement;
    return el(
      "div.item",
      {},
      el("label.item-label", {}, cb, el("span.kind", {}, kind), el("span.mono", { title: path }, label)),
      el("span.spacer"),
      el("button.icon-btn", { title: "Reveal in Finder", onclick: () => api.reveal(path).catch(toastError) }, icon(FolderSearch)),
      el("span.size", {}, formatBytes(size)),
    );
  };

  detail.replaceChildren(
    el(
      "div.detail-head",
      {},
      appIcon(a, "app-icon-big"),
      el(
        "div.app-text",
        {},
        el("h2", {}, a.name),
        el("div.dim", {}, `${a.bundleId}${a.version ? " · " + a.version : ""}`),
      ),
      el("span.spacer"),
      go,
    ),
    el(
      "div.scroll",
      {},
      el("div.label.list-label", {}, "Application"),
      el("div.section", {}, row(a.path, a.path, "App", a.size)),
      el("div.label.list-label", {}, `App data (${leftovers.length})`),
      leftovers.length
        ? el("div.section", {}, leftovers.map((l) => row(l.path, l.path, l.kind, l.size)))
        : el("div.empty.small", {}, "No app data found in your Library."),
    ),
  );
  update();
}

async function uninstall(a: AppInfo, leftovers: Leftover[], checked: Set<string>, btn: HTMLButtonElement) {
  const withApp = checked.has(a.path);
  const paths = leftovers.map((l) => l.path).filter((p) => checked.has(p));
  const total = [...checked].reduce((s, p) => s + (p === a.path ? a.size : leftovers.find((l) => l.path === p)?.size ?? 0), 0);
  const n = checked.size;
  const ok = await confirmDialog({
    title: withApp ? `Uninstall ${a.name}?` : `Remove ${a.name} data?`,
    message: `Move ${n} item${n === 1 ? "" : "s"} (${formatBytes(total)}) to the Trash.`,
    confirmLabel: "Move to Trash",
  });
  if (!ok) return;
  await busy(btn, async () => {
    const r = await api.uninstallApp(a.bundleId, a.name, withApp ? a.path : null, paths);
    if (r.failed.length) {
      toast(`${r.failed[0].reason}${r.failed.length > 1 ? ` (+${r.failed.length - 1} more failed)` : ""}`, "error");
    }
    if (r.done.length) toast(`Moved ${formatBytes(r.bytes)} to the Trash`);
    if (r.done.length) {
      if (withApp && r.done.includes(a.path)) {
        current = null;
        detail.replaceChildren(el("div.empty", {}, `${a.name} was moved to the Trash.`));
      } else {
        await select(a);
      }
      await load();
    }
  });
}
