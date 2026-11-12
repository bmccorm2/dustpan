import { el } from "../ui";

export function mount(root: HTMLElement): void {
  root.append(el("div.empty", {}, "Clean"));
}
