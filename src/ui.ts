import { createElement, type IconNode } from "lucide";

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
