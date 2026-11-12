import { listen } from "@tauri-apps/api/event";
import { Check, Lock, Play } from "lucide";
import { api, type TaskInfo, type TaskResult } from "../api";
import { busy, el, icon, toastError } from "../ui";

export function mount(root: HTMLElement): void {
  root.classList.add("optimize");
  const list = el("div.scroll");
  const run = el("button.btn.primary", {}, icon(Play), "Run selected") as HTMLButtonElement;
  root.append(
    el("header.toolbar", {}, el("div", {}, el("strong", {}, "Optimize"), el("span.dim", {}, " · quick maintenance tasks")), el("span.spacer"), run),
    list,
  );
  list.append(el("div.center", {}, el("span.spinner")));

  const checks = new Map<string, HTMLInputElement>();
  const status = new Map<string, HTMLElement>();
  const setStatus = (r: TaskResult) => {
    const box = status.get(r.id);
    if (!box) return;
    const mark =
      r.status === "running"
        ? el("span.spinner.small")
        : r.status === "done"
          ? el("span.st-done", {}, icon(Check))
          : el(`span.st-${r.status}`, {}, r.status === "skipped" ? "Skipped" : "Failed");
    box.replaceChildren(mark);
    if (r.message) box.append(el(`span.st-msg.st-${r.status}`, { title: r.message }, r.message));
  };

  const render = (tasks: TaskInfo[]) => {
    list.replaceChildren();
    if (!tasks.length) {
      list.append(el("div.empty", {}, "No tasks available."));
      return;
    }
    const section = el("div.section");
    for (const t of tasks) {
      const cb = el("input", { type: "checkbox", checked: true, onchange: sync }) as HTMLInputElement;
      checks.set(t.id, cb);
      const st = el("div.task-status");
      status.set(t.id, st);
      section.append(
        el(
          "label.task",
          {},
          cb,
          el(
            "div.task-text",
            {},
            el("div.task-title", {}, t.title, t.needsAdmin ? el("span.lock", { title: "Asks for your admin password" }, icon(Lock)) : null),
            el("div.dim", {}, t.description),
          ),
          st,
        ),
      );
    }
    list.append(section);
    sync();
  };

  function sync() {
    run.disabled = ![...checks.values()].some((c) => c.checked);
  }

  run.addEventListener("click", () =>
    busy(run, async () => {
      const ids = [...checks].filter(([, c]) => c.checked).map(([id]) => id);
      for (const id of ids) status.get(id)?.replaceChildren();
      const un = await listen<TaskResult>("optimize://progress", (e) => setStatus(e.payload));
      try {
        const results = await api.optimizeRun(ids);
        results.forEach(setStatus);
      } finally {
        un();
      }
    }),
  );

  api.optimizeTasks().then(render).catch((e) => {
    toastError(e);
    render([]);
  });
}
