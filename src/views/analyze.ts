import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AppWindow,
  Archive,
  ArrowUp,
  ChevronRight,
  Disc,
  File,
  Film,
  Folder,
  FolderOpen,
  Image,
  Music,
  RefreshCw,
  type IconNode,
} from "lucide";
import { api, type DirListing, type Entry, type EntryKind } from "../api";
import { busy, confirmDialog, el, formatBytes, icon, toast, toastError } from "../ui";

const KIND_ICON: Record<EntryKind, IconNode> = {
  folder: Folder,
  image: Image,
  video: Film,
  audio: Music,
  archive: Archive,
  diskImage: Disc,
  app: AppWindow,
  other: File,
};

let view: HTMLElement;
let crumbs: HTMLElement;
let body: HTMLElement;
let footer: HTMLElement;
let rescanBtn: HTMLButtonElement;
let chooseBtn: HTMLButtonElement;
let root = "";
let listing: DirListing | null = null;
let selectedPath: string | null = null;
let menu: HTMLElement | null = null;
let fda = true;

export function mount(r: HTMLElement): void {
  view = r;
  view.classList.add("analyze");
  crumbs = el("nav.crumbs");
  rescanBtn = el("button.btn", { onclick: () => scan(root) }, icon(RefreshCw), "Rescan") as HTMLButtonElement;
  chooseBtn = el("button.btn", { onclick: chooseFolder }, icon(FolderOpen), "Choose folder…") as HTMLButtonElement;
  const up = el("button.icon-btn", { title: "Up (Backspace)", onclick: goUp }, icon(ArrowUp));
  body = el("div.scroll.analyze-list", { tabindex: 0 });
  footer = el("footer.footer");
  view.append(el("header.toolbar", {}, up, crumbs, el("span.spacer"), chooseBtn, rescanBtn), body, footer);

  document.addEventListener("keydown", (e) => {
    if (view.closest("[hidden]") || view.hidden) return;
    if (e.target instanceof HTMLInputElement || document.querySelector("dialog[open]")) return;
    if (e.key === "Backspace") {
      e.preventDefault();
      goUp();
    } else if (e.key === "Escape") {
      closeMenu();
    }
  });
  document.addEventListener("click", closeMenu);
  window.addEventListener("blur", closeMenu);

  api.fdaStatus().then((ok) => (fda = ok)).catch(() => {});
  homeDir()
    .then((h) => scan(h))
    .catch(toastError);
}

async function chooseFolder() {
  const picked = await open({ directory: true, multiple: false, defaultPath: root || undefined });
  if (typeof picked === "string" && picked) await scan(picked);
}

async function scan(path: string) {
  root = path.replace(/\/+$/, "") || "/";
  listing = null;
  selectedPath = null;
  const progress = el("div.dim", {}, "Starting…");
  body.replaceChildren(el("div.center", {}, el("span.spinner"), el("div", {}, `Scanning ${root}`), progress));
  footer.textContent = "";
  renderCrumbs(root);
  rescanBtn.disabled = chooseBtn.disabled = true;
  const un = await listen<{ files: number; bytes: number }>("analyze://progress", (e) => {
    progress.textContent = `${e.payload.files.toLocaleString()} files · ${formatBytes(e.payload.bytes)}`;
  });
  try {
    await api.analyzeScan(root);
    await show(root);
  } catch (e) {
    toastError(e);
    body.replaceChildren(el("div.empty", {}, "Scan failed. Choose another folder or try again."));
  } finally {
    un();
    rescanBtn.disabled = chooseBtn.disabled = false;
  }
}

async function show(path: string, select?: string) {
  try {
    listing = await api.analyzeList(path);
  } catch (e) {
    toastError(e);
    return;
  }
  selectedPath = select ?? null;
  render();
}

function goUp() {
  if (!listing || listing.path === root) return;
  const parent = listing.path.slice(0, listing.path.lastIndexOf("/")) || "/";
  show(parent, listing.path);
}

function renderCrumbs(path: string) {
  crumbs.replaceChildren();
  const rel = path === root ? "" : path.slice(root.length + 1);
  const parts = rel ? rel.split("/") : [];
  const rootName = root === "/" ? "/" : root.split("/").pop() || root;
  const seg = (label: string, target: string, last: boolean) =>
    el("button.crumb", { class: last ? "last" : "", title: target, onclick: () => listing && show(target) }, label);
  crumbs.append(seg(rootName, root, parts.length === 0));
  let acc = root;
  parts.forEach((p, i) => {
    acc = acc === "/" ? `/${p}` : `${acc}/${p}`;
    crumbs.append(el("span.crumb-sep", {}, icon(ChevronRight)), seg(p, acc, i === parts.length - 1));
  });
}

function render() {
  if (!listing) return;
  renderCrumbs(listing.path);
  const l = listing;
  const max = l.entries.reduce((m, e) => Math.max(m, e.size), 0) || 1;
  body.replaceChildren();
  if (!l.entries.length) body.append(el("div.empty", {}, "This folder is empty."));
  for (const e of l.entries) {
    const row = el(
      "div.entry",
      {
        class: e.path === selectedPath ? "current" : "",
        onclick: () => select(e, row),
        ondblclick: () => e.isDir && show(e.path),
        oncontextmenu: (ev: Event) => {
          ev.preventDefault();
          select(e, row);
          openMenu(ev as MouseEvent, e);
        },
      },
      el("span.kind-icon", { class: `k-${e.kind}` }, icon(KIND_ICON[e.kind])),
      el("span.entry-name", { title: e.name }, e.name),
      el("span.bar", {}, el("span.bar-fill", { style: `width:${((e.size / max) * 100).toFixed(1)}%` })),
      el("span.size", {}, formatBytes(e.size)),
    );
    body.append(row);
    if (e.path === selectedPath) queueMicrotask(() => row.scrollIntoView({ block: "nearest" }));
  }
  const n = l.entries.length;
  footer.replaceChildren(
    el(
      "span",
      {},
      `${n.toLocaleString()} item${n === 1 ? "" : "s"} · ${formatBytes(l.total)}`,
      l.unreadable ? ` · ${l.unreadable} folder${l.unreadable === 1 ? "" : "s"} unreadable` : null,
      l.unreadable && !fda
        ? el(
            "span.fda-hint",
            {},
            " · Grant Full Disk Access to include them. ",
            el("button.link", { onclick: () => api.openFdaSettings().catch(toastError) }, "Open System Settings"),
          )
        : null,
    ),
  );
}

function select(e: Entry, row: HTMLElement) {
  selectedPath = e.path;
  for (const r of body.querySelectorAll(".entry.current")) r.classList.remove("current");
  row.classList.add("current");
}

function openMenu(ev: MouseEvent, e: Entry) {
  closeMenu();
  const item = (label: string, fn: () => void) =>
    el(
      "button.menu-item",
      {
        onclick: (x: Event) => {
          x.stopPropagation();
          closeMenu();
          fn();
        },
      },
      label,
    );
  menu = el(
    "div.popover.context-menu",
    { style: `left:${ev.clientX}px;top:${ev.clientY}px` },
    e.isDir ? item("Open", () => show(e.path)) : null,
    item("Reveal in Finder", () => api.reveal(e.path).catch(toastError)),
    item("Move to Trash…", () => trashEntry(e)),
  );
  document.body.append(menu);
  const r = menu.getBoundingClientRect();
  if (r.right > innerWidth) menu.style.left = `${innerWidth - r.width - 4}px`;
  if (r.bottom > innerHeight) menu.style.top = `${innerHeight - r.height - 4}px`;
}

function closeMenu() {
  menu?.remove();
  menu = null;
}

async function trashEntry(e: Entry) {
  const ok = await confirmDialog({
    title: `Move “${e.name}” to the Trash?`,
    message: `Moves 1 item (${formatBytes(e.size)}) to the Trash.`,
    confirmLabel: "Move to Trash",
  });
  if (!ok) return;
  await busy(rescanBtn, async () => {
    const r = await api.analyzeTrash(e.path);
    if (r.failed.length) toast(r.failed[0].reason, "error");
    else toast(`Moved ${formatBytes(r.bytes)} to the Trash`);
    if (listing) await show(listing.path);
  });
}
