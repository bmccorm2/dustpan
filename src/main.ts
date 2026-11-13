import "./theme.css";
import "./styles.css";
import { Gauge, HardDrive, Package, Sparkles, type IconNode } from "lucide";
import { el, fdaBanner, icon } from "./ui";
import * as clean from "./views/clean";
import * as apps from "./views/apps";
import * as optimize from "./views/optimize";
import * as analyze from "./views/analyze";

interface Tab {
  id: string;
  label: string;
  icon: IconNode;
  view: { mount(root: HTMLElement): void };
}

const TABS: Tab[] = [
  { id: "clean", label: "Clean", icon: Sparkles, view: clean },
  { id: "apps", label: "Apps", icon: Package, view: apps },
  { id: "optimize", label: "Optimize", icon: Gauge, view: optimize },
  { id: "analyze", label: "Analyze", icon: HardDrive, view: analyze },
];
const KEY = "dustpan.tab";

const app = document.getElementById("app")!;
const bar = el("nav.tabbar");
const views = el("div.views");
app.append(bar, fdaBanner(), views);

const panes = new Map<string, HTMLElement>();
const buttons = new Map<string, HTMLElement>();

function show(id: string) {
  const tab = TABS.find((t) => t.id === id) ?? TABS[0];
  localStorage.setItem(KEY, tab.id);
  let pane = panes.get(tab.id);
  if (!pane) {
    // Mount lazily on first visit, then keep the view alive so state survives tab switches.
    pane = el("section.view");
    panes.set(tab.id, pane);
    views.append(pane);
    tab.view.mount(pane);
  }
  for (const [k, p] of panes) p.hidden = k !== tab.id;
  for (const [k, b] of buttons) b.classList.toggle("active", k === tab.id);
}

for (const t of TABS) {
  const b = el("button.tab", { onclick: () => show(t.id) }, icon(t.icon), el("span", {}, t.label));
  buttons.set(t.id, b);
  bar.append(b);
}

show(localStorage.getItem(KEY) ?? "clean");
