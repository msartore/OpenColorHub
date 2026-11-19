/**
 * Tiny, dependency-free Markdown renderer for release notes.
 *
 * Deliberately a subset: headings, tables, lists, quotes, fenced code, bold,
 * italic, inline code and links. Everything is built as DOM nodes with
 * `textContent`, so untrusted release notes can never inject HTML.
 */

import { el } from "./dom";

function inline(text: string): Node[] {
  const nodes: Node[] = [];
  const code = /`([^`]+)`/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = code.exec(text)) !== null) {
    nodes.push(...emphasis(text.slice(cursor, match.index)));
    nodes.push(el("code", { class: "md-code" }, match[1]));
    cursor = match.index + match[0].length;
  }
  nodes.push(...emphasis(text.slice(cursor)));
  return nodes;
}

function emphasis(text: string): Node[] {
  const nodes: Node[] = [];
  const token = /(\*\*[^*]+\*\*|\*[^*]+\*|\[[^\]]+\]\([^)]+\))/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = token.exec(text)) !== null) {
    nodes.push(document.createTextNode(text.slice(cursor, match.index)));
    const part = match[0];
    if (part.startsWith("**") && part.endsWith("**")) {
      nodes.push(el("strong", {}, part.slice(2, -2)));
    } else if (part.startsWith("*") && part.endsWith("*")) {
      nodes.push(el("em", {}, part.slice(1, -1)));
    } else if (part.startsWith("[")) {
      const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(part);
      if (link) {
        const href = safeHref(link[2]);
        nodes.push(
          href
            ? el("a", { href, target: "_blank", rel: "noopener noreferrer" }, link[1])
            : document.createTextNode(link[1]),
        );
      } else {
        nodes.push(document.createTextNode(part));
      }
    }
    cursor = match.index + part.length;
  }
  nodes.push(document.createTextNode(text.slice(cursor)));
  return nodes;
}

function safeHref(url: string): string | null {
  const trimmed = url.trim();
  return /^(https?:\/\/|mailto:)/i.test(trimmed) ? trimmed : null;
}

function cells(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((part) => part.trim());
}

function isSeparator(line: string): boolean {
  const parts = cells(line);
  return parts.length > 0 && parts.every((part) => /^:?-{3,}:?$/.test(part));
}

/** Renders a Markdown string into a scrollable container element. */
export function renderMarkdown(source: string): HTMLElement {
  const body = el("div", { class: "md-body" });
  const lines = source.replace(/\r\n/g, "\n").split("\n");
  let i = 0;

  const paragraph = (): void => {
    const parts: string[] = [];
    while (i < lines.length && lines[i].trim() !== "" && !isBlockStart(lines[i])) {
      parts.push(lines[i].trim());
      i += 1;
    }
    if (parts.length === 0) return;
    const p = el("p", { class: "md-p" });
    parts.forEach((part, index) => {
      if (index > 0) p.appendChild(el("br"));
      p.append(...inline(part));
    });
    body.appendChild(p);
  };

  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "") {
      i += 1;
      continue;
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      const level = Math.min(heading[1].length + 1, 6);
      const tag = `h${level}` as "h2" | "h3" | "h4" | "h5" | "h6";
      body.appendChild(el(tag, { class: "md-heading" }, ...inline(heading[2])));
      i += 1;
      continue;
    }

    if (line.trim().startsWith("```")) {
      const code: string[] = [];
      i += 1;
      while (i < lines.length && !lines[i].trim().startsWith("```")) {
        code.push(lines[i]);
        i += 1;
      }
      i += 1;
      body.appendChild(el("pre", { class: "md-pre" }, code.join("\n")));
      continue;
    }

    if (/^\s*(-{3,}|\*{3,})\s*$/.test(line)) {
      body.appendChild(el("hr", { class: "md-hr" }));
      i += 1;
      continue;
    }

    if (line.trim().startsWith("|") && i + 1 < lines.length && isSeparator(lines[i + 1])) {
      const head = cells(line);
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && lines[i].trim().startsWith("|")) {
        rows.push(cells(lines[i]));
        i += 1;
      }
      const wrap = el("div", { class: "md-table-wrap" });
      const table = el("table", { class: "md-table" });
      const thead = el("thead");
      const headRow = el("tr");
      head.forEach((cell) => headRow.appendChild(el("th", {}, ...inline(cell))));
      thead.appendChild(headRow);
      const tbody = el("tbody");
      for (const row of rows) {
        const tr = el("tr");
        head.forEach((_, index) =>
          tr.appendChild(el("td", {}, ...inline(row[index] ?? ""))),
        );
        tbody.appendChild(tr);
      }
      table.append(thead, tbody);
      wrap.appendChild(table);
      body.appendChild(wrap);
      continue;
    }

    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    if (bullet) {
      const list = el("ul", { class: "md-list" });
      while (i < lines.length) {
        const item = /^\s*[-*]\s+(.*)$/.exec(lines[i]);
        if (!item) break;
        list.appendChild(el("li", {}, ...inline(item[1])));
        i += 1;
      }
      body.appendChild(list);
      continue;
    }

    const numbered = /^\s*\d+\.\s+(.*)$/.exec(line);
    if (numbered) {
      const list = el("ol", { class: "md-list" });
      while (i < lines.length) {
        const item = /^\s*\d+\.\s+(.*)$/.exec(lines[i]);
        if (!item) break;
        list.appendChild(el("li", {}, ...inline(item[1])));
        i += 1;
      }
      body.appendChild(list);
      continue;
    }

    const quote = /^>\s?(.*)$/.exec(line);
    if (quote) {
      const parts: string[] = [];
      while (i < lines.length) {
        const item = /^>\s?(.*)$/.exec(lines[i]);
        if (!item) break;
        parts.push(item[1]);
        i += 1;
      }
      body.appendChild(el("blockquote", { class: "md-quote" }, parts.join(" ")));
      continue;
    }

    paragraph();
  }

  return body;
}

function isBlockStart(line: string): boolean {
  return (
    /^#{1,6}\s/.test(line) ||
    line.trim().startsWith("```") ||
    line.trim().startsWith("|") ||
    /^\s*[-*]\s/.test(line) ||
    /^\s*\d+\.\s/.test(line) ||
    /^>\s?/.test(line)
  );
}
